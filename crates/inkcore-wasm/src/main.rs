//! `inkcore::Core` for the web page: a small C ABI with JSON commands, built by `scripts/build-web.sh` as an
//! emscripten ES module and wrapped by `web/js/core.js`. On other targets this is an empty program.

fn main() {}

#[cfg(target_os = "emscripten")]
mod abi {
    use std::cell::{Cell, RefCell};
    use std::collections::{BTreeMap, HashMap};

    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use inkcore::{AppFiles, Config, Core, NetMode, NetResult, Persisted, DEFAULT_DELAY_MS};
    use serde_json::{json, Map, Value};

    thread_local! {
        static CORES: RefCell<HashMap<i32, Core>> = RefCell::new(HashMap::new());
        static NEXT_HANDLE: Cell<i32> = const { Cell::new(1) };
        /// The last `ik_cmd` result or `ik_last_error` message; `ik_result_len` is its length.
        static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        /// Kept apart from `RESULT`, so reading a frame survives other calls.
        static FRAME: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
    }

    const FONT_NAMES: [&str; 3] = ["small.bdf", "medium.bdf", "large.bdf"];

    /// # Safety
    /// `ptr` points to `len` readable bytes (any pointer when `len` is 0).
    unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
        if len == 0 || ptr.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts(ptr, len)
        }
    }

    fn set_result(bytes: Vec<u8>) -> *const u8 {
        RESULT.with_borrow_mut(|r| {
            *r = bytes;
            r.as_ptr()
        })
    }

    fn fail(msg: String) {
        LAST_ERROR.with_borrow_mut(|e| *e = msg);
    }

    /// The blob's entries: repeated `[u32 LE name_len][name][u32 LE len][bytes]`.
    fn entries(blob: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
        let mut out = Vec::new();
        let mut i = 0;
        let take = |i: &mut usize| -> Result<&[u8], String> {
            let truncated = || format!("blob: truncated at byte {i}", i = *i);
            let n = blob.get(*i..*i + 4).ok_or_else(truncated)?;
            let n = u32::from_le_bytes(n.try_into().unwrap()) as usize;
            let data = blob
                .get(*i + 4..(*i + 4).saturating_add(n))
                .ok_or_else(truncated)?;
            *i += 4 + n;
            Ok(data)
        };
        while i < blob.len() {
            let name = take(&mut i)?;
            let name = String::from_utf8(name.to_vec())
                .map_err(|_| "blob: an entry name is not UTF-8".to_string())?;
            let data = take(&mut i)?.to_vec();
            out.push((name, data));
        }
        Ok(out)
    }

    /// A blob sorted by kind. `fixtures/<path>` other than `net/<name>` and `reader.json` is ignored.
    #[derive(Default)]
    struct Blob {
        app: BTreeMap<String, Vec<u8>>,
        net: BTreeMap<String, Vec<u8>>,
        has_fixtures: bool,
        reader: Option<Vec<u8>>,
        fonts: BTreeMap<String, Vec<u8>>,
    }

    fn parse_blob(blob: &[u8]) -> Result<Blob, String> {
        let mut b = Blob::default();
        for (name, data) in entries(blob)? {
            if let Some(path) = name.strip_prefix("app/").filter(|p| !p.is_empty()) {
                b.app.insert(path.to_string(), data);
            } else if let Some(path) = name.strip_prefix("fixtures/").filter(|p| !p.is_empty()) {
                b.has_fixtures = true;
                if path == "reader.json" {
                    b.reader = Some(data);
                } else if let Some(file) = path.strip_prefix("net/") {
                    if !file.is_empty() && !file.contains('/') {
                        b.net.insert(file.to_string(), data);
                    }
                }
            } else if let Some(font) = name
                .strip_prefix("fonts/")
                .filter(|f| FONT_NAMES.contains(f))
            {
                b.fonts.insert(font.to_string(), data);
            } else {
                return Err(format!("blob: unknown entry {name:?}"));
            }
        }
        Ok(b)
    }

    /// The letters of the folder name, as the native shell's fallback app id.
    fn fallback_app_id(label: &str) -> String {
        let name = label.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        let letters: String = name.chars().filter(char::is_ascii_alphabetic).collect();
        if letters.is_empty() {
            "app".into()
        } else {
            letters
        }
    }

    fn config(json: &[u8], b: &mut Blob) -> Result<Config, String> {
        let v: Value = serde_json::from_slice(json).map_err(|e| format!("config: {e}"))?;
        let o = v.as_object().ok_or("config: not a JSON object")?;
        let int = |k: &str| {
            o.get(k)
                .and_then(Value::as_i64)
                .ok_or(format!("config: {k} must be an integer"))
        };
        let text = |k: &str| {
            o.get(k)
                .and_then(Value::as_str)
                .ok_or(format!("config: {k} must be a string"))
        };
        let net_mode = match text("net")? {
            "fixtures+live" => NetMode::FixturesThenLive,
            "replay" => NetMode::Replay,
            other => {
                return Err(format!(
                    "config: net must be \"fixtures+live\" or \"replay\", not {other:?}"
                ))
            }
        };
        let random_seed = match o.get("random_seed") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                v.as_u64()
                    .or_else(|| v.as_i64().map(|n| n as u64))
                    .ok_or("config: random_seed must be an integer or null")?,
            ),
        };
        let fonts = match b.fonts.len() {
            0 => None,
            3 => Some(FONT_NAMES.map(|n| b.fonts.remove(n).unwrap_or_default())),
            _ => {
                return Err(format!(
                    "fonts: give all three of {} or none",
                    FONT_NAMES.join(", ")
                ))
            }
        };
        let label = text("app_label")?;
        Ok(Config {
            start_local_ms: int("start_local_ms")?,
            utc_offset_sec: i32::try_from(int("utc_offset_sec")?)
                .map_err(|_| "config: utc_offset_sec is out of range")?,
            net_mode,
            random_seed,
            locale: text("locale")?.to_string(),
            app_dir_label: label.to_string(),
            fallback_app_id: fallback_app_id(label),
            reader_json: b.reader.take(),
            fixtures: std::mem::take(&mut b.net),
            net_fixtures_label: "fixtures/net".into(),
            fonts,
            ..Config::default()
        })
    }

    fn create(blob: &[u8], cfg: &[u8]) -> Result<Core, String> {
        let mut b = parse_blob(blob)?;
        let cfg = config(cfg, &mut b)?;
        Ok(Core::new(AppFiles(b.app), cfg, Persisted::default()))
    }

    /// Creates a core from the app/fixture/font blob and the config JSON: a handle >= 1, or 0 with the
    /// message in `ik_last_error`.
    ///
    /// # Safety
    /// `blob` and `cfg` point to `blob_len` and `cfg_len` readable bytes.
    #[no_mangle]
    pub unsafe extern "C" fn ik_create(
        blob: *const u8,
        blob_len: usize,
        cfg: *const u8,
        cfg_len: usize,
    ) -> i32 {
        match create(slice(blob, blob_len), slice(cfg, cfg_len)) {
            Ok(core) => {
                let h = NEXT_HANDLE.get();
                NEXT_HANDLE.set(h + 1);
                CORES.with_borrow_mut(|m| m.insert(h, core));
                h
            }
            Err(e) => {
                fail(e);
                0
            }
        }
    }

    #[no_mangle]
    pub extern "C" fn ik_destroy(handle: i32) {
        let core = CORES.with_borrow_mut(|m| m.remove(&handle));
        drop(core);
    }

    #[no_mangle]
    pub extern "C" fn ik_result_len() -> usize {
        RESULT.with_borrow(Vec::len)
    }

    #[no_mangle]
    pub extern "C" fn ik_last_error() -> *const u8 {
        set_result(LAST_ERROR.with_borrow(|e| e.clone().into_bytes()))
    }

    /// Runs one JSON command; the JSON result stays readable until the next call.
    ///
    /// # Safety
    /// `json` points to `len` readable bytes.
    #[no_mangle]
    pub unsafe extern "C" fn ik_cmd(handle: i32, json: *const u8, len: usize) -> *const u8 {
        // held while Lua runs: no synchronous JS callback may ever call back into `ik_*`
        let out = CORES.with_borrow_mut(|m| match m.get_mut(&handle) {
            Some(core) => run(core, slice(json, len)),
            None => json!({"ok": false, "error": format!("unknown core handle {handle}")}),
        });
        set_result(out.to_string().into_bytes())
    }

    /// The screen in PIL mode "1" (rows of `ceil(w / 8)` bytes): the app's own frame when `dialog` is 0, with
    /// every plate (error, dialog, toast) otherwise; null for an unknown handle.
    #[no_mangle]
    pub extern "C" fn ik_frame(handle: i32, dialog: i32) -> *const u8 {
        let Some(bits) = CORES.with_borrow(|m| {
            m.get(&handle).map(|c| {
                if dialog == 0 {
                    c.raw_frame().packed()
                } else {
                    c.frame(true).packed()
                }
            })
        }) else {
            return std::ptr::null();
        };
        FRAME.with_borrow_mut(|f| {
            *f = bits;
            f.as_ptr()
        })
    }

    #[no_mangle]
    pub extern "C" fn ik_frame_w(handle: i32) -> u32 {
        CORES.with_borrow(|m| m.get(&handle).map_or(0, |c| c.status().screen.0))
    }

    #[no_mangle]
    pub extern "C" fn ik_frame_h(handle: i32) -> u32 {
        CORES.with_borrow(|m| m.get(&handle).map_or(0, |c| c.status().screen.1))
    }

    // ---- commands ---------------------------------------------------------------------------------

    type Req = Map<String, Value>;

    fn run(core: &mut Core, json: &[u8]) -> Value {
        let req: Req = match serde_json::from_slice(json) {
            Ok(Value::Object(o)) => o,
            Ok(_) => return json!({"ok": false, "error": "a command must be a JSON object"}),
            Err(e) => return json!({"ok": false, "error": format!("command: {e}")}),
        };
        let cmd = req.get("cmd").and_then(Value::as_str).unwrap_or_default();
        let mut out = match command(core, cmd, &req) {
            Ok(mut extra) => {
                extra.insert("ok".into(), true.into());
                extra
            }
            Err(e) => Map::from_iter([("ok".into(), false.into()), ("error".into(), e.into())]),
        };
        if cmd == "state" || req.get("with_state") == Some(&Value::Bool(true)) {
            let from = req.get("log_from").and_then(Value::as_u64).unwrap_or(0);
            out.extend(state(core, usize::try_from(from).unwrap_or(usize::MAX)));
        }
        Value::Object(out)
    }

    fn get<'a>(req: &'a Req, cmd: &str, k: &str) -> Result<&'a Value, String> {
        req.get(k).ok_or(format!("{cmd}: missing {k}"))
    }

    fn int(req: &Req, cmd: &str, k: &str) -> Result<i64, String> {
        get(req, cmd, k)?
            .as_i64()
            .ok_or(format!("{cmd}: {k} must be an integer"))
    }

    fn i32_(req: &Req, cmd: &str, k: &str) -> Result<i32, String> {
        i32::try_from(int(req, cmd, k)?).map_err(|_| format!("{cmd}: {k} is out of range"))
    }

    fn text<'a>(req: &'a Req, cmd: &str, k: &str) -> Result<&'a str, String> {
        get(req, cmd, k)?
            .as_str()
            .ok_or(format!("{cmd}: {k} must be a string"))
    }

    fn flag(req: &Req, cmd: &str, k: &str) -> Result<bool, String> {
        get(req, cmd, k)?
            .as_bool()
            .ok_or(format!("{cmd}: {k} must be true or false"))
    }

    fn b64(cmd: &str, k: &str, v: &Value) -> Result<Vec<u8>, String> {
        let s = v.as_str().ok_or(format!("{cmd}: {k} must be a string"))?;
        B64.decode(s).map_err(|e| format!("{cmd}: {k}: {e}"))
    }

    fn command(core: &mut Core, cmd: &str, req: &Req) -> Result<Req, String> {
        let mut out = Req::new();
        match cmd {
            "pointer" => core.pointer(
                text(req, cmd, "phase")?,
                i32_(req, cmd, "x")?,
                i32_(req, cmd, "y")?,
            )?,
            "tap" => core.tap(i32_(req, cmd, "x")?, i32_(req, cmd, "y")?)?,
            "double_tap" => core.double_tap(i32_(req, cmd, "x")?, i32_(req, cmd, "y")?)?,
            "key" => core.key(text(req, cmd, "name")?)?,
            "type_text" => core.type_text(text(req, cmd, "text")?)?,
            "submit_text" => core.submit_text(text(req, cmd, "text")?)?,
            "answer" => core.answer(flag(req, cmd, "ok")?)?,
            "wait" => core.wait(int(req, cmd, "ms")?)?,
            "advance" => core.advance(int(req, cmd, "ms")?)?,
            "lock" => core.lock(),
            "unlock" => core.unlock(),
            "reload" => {
                if let Some(v) = req.get("blob") {
                    let b = parse_blob(&b64(cmd, "blob", v)?)?;
                    core.set_app(AppFiles(b.app));
                    if b.has_fixtures {
                        core.set_fixtures(b.net);
                    }
                }
                core.reload();
            }
            "restart" => core.restart(),
            "set_paused" => core.set_paused(flag(req, cmd, "paused")?),
            "take_outgoing" => {
                let requests: Vec<Value> = core
                    .take_outgoing()
                    .into_iter()
                    .map(|r| {
                        json!({
                            "id": r.id,
                            "method": r.method,
                            "url": r.url,
                            "headers": r.headers,
                            "body_b64": B64.encode(&r.body),
                            "timeout_ms": r.timeout_ms,
                        })
                    })
                    .collect();
                out.insert("requests".into(), requests.into());
            }
            "deliver" => {
                let id = get(req, cmd, "id")?
                    .as_u64()
                    .ok_or("deliver: id must be an integer")?;
                let result = deliver_result(req)?;
                core.deliver(id, result);
            }
            "state" => {}
            "" => return Err("missing cmd".into()),
            other => return Err(format!("unknown command {other:?}")),
        }
        Ok(out)
    }

    fn deliver_result(req: &Req) -> Result<NetResult, String> {
        const CMD: &str = "deliver";
        if let Some(e) = req.get("error").filter(|v| !v.is_null()) {
            let e = e.as_str().ok_or("deliver: error must be a string")?;
            return Ok(NetResult::Error(e.to_string()));
        }
        let status = match req.get("status") {
            None => 200,
            Some(_) => int(req, CMD, "status")?,
        };
        let delay_ms = match req.get("delay_ms") {
            None => DEFAULT_DELAY_MS,
            Some(_) => int(req, CMD, "delay_ms")?,
        };
        let body = match req.get("body_b64") {
            None => Vec::new(),
            Some(v) => b64(CMD, "body_b64", v)?,
        };
        let bad_headers = || "deliver: headers must be a list of [name, value] strings".to_string();
        let headers = match req.get("headers") {
            None => Vec::new(),
            Some(v) => v
                .as_array()
                .ok_or_else(bad_headers)?
                .iter()
                .map(|pair| match pair.as_array().map(Vec::as_slice) {
                    Some([Value::String(k), Value::String(v)]) => Ok((k.to_lowercase(), v.clone())),
                    _ => Err(bad_headers()),
                })
                .collect::<Result<_, _>>()?,
        };
        Ok(NetResult::Response {
            status,
            headers,
            body,
            delay_ms,
        })
    }

    fn state(core: &Core, log_from: usize) -> Req {
        // first: frame_seq borrows the core mutably, so no log()/errors() Ref may be alive
        let frame_seq = core.frame_seq();
        let st = core.status();
        let texts: Vec<Value> = core
            .texts()
            .into_iter()
            .map(
                |t| json!({"text": t.text, "x": t.x, "y": t.y, "w": t.w, "h": t.h, "font": t.font}),
            )
            .collect();
        let errors = core.errors().to_vec();
        let warnings_count = core.warnings().len();
        let (log_from, log) = {
            let log = core.log();
            let from = log_from.min(log.len());
            (from, log[from..].to_vec())
        };
        let status = json!({
            "time": st.time,
            "net": st.net,
            "frame_commands": st.frame_commands,
            "paused": st.paused,
            "screen": [st.screen.0, st.screen.1],
            "app_id": st.app_id,
            "running": st.running,
            "pending": st.pending.map(|p| json!({"kind": p.kind, "title": p.title, "text": p.text})),
            "locked": st.locked,
            "refresh": st.refresh,
            "refreshes": st.refreshes,
        });
        Map::from_iter([
            ("status".into(), status),
            ("frame_seq".into(), frame_seq.into()),
            ("texts".into(), texts.into()),
            ("errors".into(), errors.into()),
            ("warnings_count".into(), warnings_count.into()),
            ("log_from".into(), log_from.into()),
            ("log".into(), log.into()),
        ])
    }
}
