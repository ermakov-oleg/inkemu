//! The stdio MCP server of 0.1 `mcp_server.py`: starts or finds the daemon and forwards commands to it over HTTP.
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::service::ServerInitializeError;
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::command::Reply;
use crate::{daemon, paths};
use inkcore::log::py_repr_str;

const START_TIMEOUT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(100);
const NET_MODES: [&str; 3] = ["replay", "record", "live"];
const NO_DAEMON: &str = "no emulator running; call emu_start first";

fn failure(msg: impl Into<String>) -> Reply {
    Reply {
        ok: Some(false),
        error: Some(Some(msg.into())),
        ..Reply::default()
    }
}

fn post(cmd: &str, args: Value) -> Reply {
    let Some(info) = daemon::read_info(&paths::state_root()) else {
        return failure(NO_DAEMON);
    };
    let args = match args {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    match daemon::post_command(info.port, cmd, &args) {
        Ok(v) => Reply::from_value(v),
        Err(e) => failure(e),
    }
}

/// `{**state, "already_running": True}`, with `error` appended only when `state` had none.
#[derive(Debug, Serialize)]
struct Attached {
    #[serde(flatten)]
    state: Reply,
    already_running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct Started {
    #[serde(flatten)]
    state: Reply,
    pid: u32,
    port: u16,
}

/// The state of a running daemon for the same app, or ok=false if it cannot give what was asked for.
fn attach_reply(
    mut state: Reply,
    live_window: bool,
    window: Option<bool>,
    net: Option<&str>,
    fresh: bool,
) -> Attached {
    let have_net = state.status.as_ref().map(|s| s.net.clone());
    let have = [
        (
            "net",
            have_net.as_deref().map_or("null".into(), str::to_owned),
        ),
        ("window", live_window.to_string()),
    ];
    let mut asked = Vec::new();
    if let Some(n) = net {
        if have_net.as_deref() != Some(n) {
            asked.push(("net", n.to_owned()));
        }
    }
    if let Some(w) = window {
        if w != live_window {
            asked.push(("window", w.to_string()));
        }
    }
    if fresh {
        asked.push(("fresh", "true".to_owned()));
    }
    if asked.is_empty() {
        return Attached {
            state,
            already_running: true,
            error: None,
        };
    }
    let join = |kv: &[(&str, String)]| {
        kv.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let msg = format!(
        "a daemon for this app is already running ({}) and cannot switch to {}; \
         call emu_stop first, then emu_start again",
        join(&have),
        join(&asked)
    );
    state.ok = Some(false);
    let error = if state.error.is_some() {
        state.error = Some(Some(msg));
        None
    } else {
        Some(msg)
    };
    Attached {
        state,
        already_running: true,
        error,
    }
}

fn check_net(net: Option<&str>) -> Result<(), String> {
    match net {
        Some(n) if !NET_MODES.contains(&n) => Err(format!(
            "net must be one of {}, got {}",
            NET_MODES.join(", "),
            Value::String(n.to_owned())
        )),
        _ => Ok(()),
    }
}

/// The last `n` characters of `s`, as Python's `s[-n:]`.
fn tail_chars(s: &str, n: usize) -> &str {
    let count = s.chars().count();
    if count <= n {
        return s;
    }
    let skip = s.char_indices().nth(count - n).map_or(0, |(i, _)| i);
    &s[skip..]
}

fn json_result<T: Serialize>(v: &T) -> CallToolResult {
    match serde_json::to_string_pretty(v) {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
    }
}

async fn blocking<T, F>(f: F) -> CallToolResult
where
    T: Serialize + Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(v) => json_result(&v),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum StartReply {
    Plain(Reply),
    Attached(Attached),
    Started(Started),
}

struct StartRequest {
    python: Option<PathBuf>,
    app: String,
    window: Option<bool>,
    net: Option<String>,
    fresh: bool,
    port: i64,
}

fn start(req: StartRequest) -> StartReply {
    if let Err(e) = check_net(req.net.as_deref()) {
        return StartReply::Plain(failure(e));
    }
    let root = paths::state_root();
    if let Some(live) = daemon::read_info(&root) {
        if live.app != paths::app_id_of(Path::new(&req.app), None) {
            return StartReply::Plain(failure(format!(
                "a daemon for app {} is already running (pid {}); call emu_stop first",
                py_repr_str(&live.app),
                live.pid
            )));
        }
        let state = post("state", Value::Null);
        return StartReply::Attached(attach_reply(
            state,
            live.window,
            req.window,
            req.net.as_deref(),
            req.fresh,
        ));
    }
    match spawn(&req, &root) {
        Ok(started) => StartReply::Started(started),
        Err(e) => StartReply::Plain(failure(format!("the daemon did not start: {e}"))),
    }
}

fn spawn(req: &StartRequest, root: &Path) -> Result<Started, String> {
    let mut cmd = match &req.python {
        Some(py) => {
            let mut c = Command::new(py);
            c.args(["-m", "inkemu"]);
            c
        }
        None => Command::new(std::env::current_exe().map_err(|e| e.to_string())?),
    };
    cmd.arg(if req.window == Some(true) {
        "window"
    } else {
        "serve"
    })
    .arg(&req.app)
    .args(["--net", req.net.as_deref().unwrap_or("replay")])
    .args(["--port", &req.port.to_string()]);
    if req.fresh {
        cmd.arg("--fresh");
    }
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut err = tempfile::tempfile().map_err(|e| e.to_string())?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(err.try_clone().map_err(|e| e.to_string())?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe; the daemon must outlive this server's session.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(info) = daemon::read_info(root) {
            if info.pid == child.id() {
                // reaped here so a dead daemon is not a zombie that read_info takes for alive
                std::thread::spawn(move || child.wait());
                return Ok(Started {
                    state: post("state", Value::Null),
                    pid: info.pid,
                    port: info.port,
                });
            }
        }
        if !matches!(child.try_wait(), Ok(None)) {
            break;
        }
        std::thread::sleep(POLL);
    }
    if !matches!(child.try_wait(), Ok(Some(_))) {
        let _ = child.kill();
        let _ = child.wait();
    }
    let mut bytes = Vec::new();
    let _ = err.seek(SeekFrom::Start(0));
    let _ = err.read_to_end(&mut bytes);
    Err(tail_chars(&String::from_utf8_lossy(&bytes), 2000).to_owned())
}

fn stop() -> Reply {
    let pid = daemon::read_info(&paths::state_root()).map(|i| i.pid);
    let result = post("stop", Value::Null);
    if result.ok == Some(true) && pid.is_some_and(|p| p != 0) {
        for _ in 0..100 {
            if daemon::read_info(&paths::state_root()).is_none() {
                break;
            }
            std::thread::sleep(POLL);
        }
    }
    result
}

fn screenshot() -> Result<Vec<u8>, String> {
    let tmp = tempfile::Builder::new()
        .prefix("inkemu-shot-")
        .suffix(".png")
        .tempfile_in(std::env::temp_dir())
        .map_err(|e| e.to_string())?
        .into_temp_path();
    let result = post("shot", json!({"path": tmp.to_string_lossy()}));
    let shot = matches!(&result.shot, Some(Some(s)) if !s.is_empty());
    if result.ok != Some(true) || !shot {
        return Err(match result.error {
            Some(Some(e)) if !e.is_empty() => e,
            _ => "no screenshot".into(),
        });
    }
    std::fs::read(&tmp).map_err(|e| e.to_string())
}

const START_DOC: &str = "Start the emulator daemon for an app directory: headless by default, or with a window; \
net replay (default),\nrecord or live. Reuses a running daemon for the same app unless window/net/fresh ask for \
something else.\nport 0 picks a free port.";

fn default_ms() -> i64 {
    800
}

#[derive(Debug, Deserialize, JsonSchema)]
struct StartArgs {
    app: String,
    #[serde(default)]
    window: Option<bool>,
    #[serde(default)]
    net: Option<String>,
    #[serde(default)]
    fresh: bool,
    #[serde(default)]
    port: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct XyArgs {
    x: i64,
    y: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct HoldArgs {
    x: i64,
    y: i64,
    #[serde(default = "default_ms")]
    ms: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TextArgs {
    text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DirectionArgs {
    direction: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct KeyArgs {
    key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AnswerArgs {
    ok: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DurationArgs {
    duration: String,
}

#[derive(Clone)]
struct Server {
    python: Option<PathBuf>,
}

async fn forward(cmd: &'static str, args: Value) -> CallToolResult {
    blocking(move || post(cmd, args)).await
}

#[tool_router]
impl Server {
    #[tool(description = START_DOC)]
    async fn emu_start(&self, Parameters(a): Parameters<StartArgs>) -> CallToolResult {
        let req = StartRequest {
            python: self.python.clone(),
            app: a.app,
            window: a.window,
            net: a.net,
            fresh: a.fresh,
            port: a.port,
        };
        blocking(move || start(req)).await
    }

    #[tool(description = "Tap at screen coordinates.")]
    async fn emu_tap(&self, Parameters(a): Parameters<XyArgs>) -> CallToolResult {
        forward("tap", json!({"x": a.x, "y": a.y})).await
    }

    #[tool(
        description = "Tap the centre of the visible text (the last drawn one if it occurs several times)."
    )]
    async fn emu_tap_text(&self, Parameters(a): Parameters<TextArgs>) -> CallToolResult {
        forward("tap-text", json!({"text": a.text})).await
    }

    #[tool(description = "Double tap at screen coordinates.")]
    async fn emu_double_tap(&self, Parameters(a): Parameters<XyArgs>) -> CallToolResult {
        forward("double-tap", json!({"x": a.x, "y": a.y})).await
    }

    #[tool(description = "Swipe: up, down, left or right.")]
    async fn emu_swipe(&self, Parameters(a): Parameters<DirectionArgs>) -> CallToolResult {
        forward("swipe", json!({"direction": a.direction})).await
    }

    #[tool(description = "Press and hold at coordinates for ms milliseconds.")]
    async fn emu_hold(&self, Parameters(a): Parameters<HoldArgs>) -> CallToolResult {
        forward("hold", json!({"x": a.x, "y": a.y, "ms": a.ms})).await
    }

    #[tool(description = "Press a side button or key: up, down, left, right, ok, back.")]
    async fn emu_key(&self, Parameters(a): Parameters<KeyArgs>) -> CallToolResult {
        forward("key", json!({"key": a.key})).await
    }

    #[tool(description = "Type text into the open input dialog.")]
    async fn emu_type(&self, Parameters(a): Parameters<TextArgs>) -> CallToolResult {
        forward("type", json!({"text": a.text})).await
    }

    #[tool(description = "Answer a confirm dialog: true = yes, false = no.")]
    async fn emu_answer(&self, Parameters(a): Parameters<AnswerArgs>) -> CallToolResult {
        forward("answer", json!({"answer": a.ok})).await
    }

    #[tool(description = "Advance virtual time, e.g. '500ms', '61s', '2m'.")]
    async fn emu_wait(&self, Parameters(a): Parameters<DurationArgs>) -> CallToolResult {
        forward("wait", json!({"duration": a.duration})).await
    }

    #[tool(description = "Lock the device (lock screen).")]
    async fn emu_lock(&self) -> CallToolResult {
        forward("lock", Value::Null).await
    }

    #[tool(description = "Unlock the device.")]
    async fn emu_unlock(&self) -> CallToolResult {
        forward("unlock", Value::Null).await
    }

    #[tool(description = "Reload the app code, keeping its state.")]
    async fn emu_reload(&self) -> CallToolResult {
        forward("reload", Value::Null).await
    }

    #[tool(description = "Restart the app from scratch.")]
    async fn emu_restart(&self) -> CallToolResult {
        forward("restart", Value::Null).await
    }

    #[tool(description = "Screenshot of the current screen as a PNG image.")]
    async fn emu_screenshot(&self) -> CallToolResult {
        let shot = tokio::task::spawn_blocking(screenshot)
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        match shot {
            Ok(png) => CallToolResult::success(vec![ContentBlock::image(
                base64::engine::general_purpose::STANDARD.encode(png),
                "image/png",
            )]),
            Err(e) => CallToolResult::error(vec![ContentBlock::text(e)]),
        }
    }

    #[tool(description = "Visible texts with coordinates, plus errors, warnings and status.")]
    async fn emu_texts(&self) -> CallToolResult {
        forward("texts", Value::Null).await
    }

    #[tool(description = "The full emulator log.")]
    async fn emu_log(&self) -> CallToolResult {
        forward("log", Value::Null).await
    }

    #[tool(description = "Let the virtual clock run again after remote commands froze it.")]
    async fn emu_resume(&self) -> CallToolResult {
        forward("resume", Value::Null).await
    }

    #[tool(description = "Stop the daemon (and its window).")]
    async fn emu_stop(&self) -> CallToolResult {
        blocking(stop).await
    }
}

#[tool_handler(name = "inkemu")]
impl ServerHandler for Server {}

/// Serves MCP on stdin/stdout until stdin closes. `python` starts the daemon as `python -m inkemu`,
/// None as this executable.
pub fn main(python: Option<PathBuf>) -> i32 {
    // a runtime of our own: this also runs inside the Python extension
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("inkemu-mcp: {e}");
            return 1;
        }
    };
    let code = rt.block_on(async move {
        match (Server { python }).serve(rmcp::transport::stdio()).await {
            Ok(running) => match running.waiting().await {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("inkemu-mcp: {e}");
                    1
                }
            },
            Err(ServerInitializeError::ConnectionClosed(_)) => 0,
            Err(e) => {
                eprintln!("inkemu-mcp: {e}");
                1
            }
        }
    });
    // a tool still blocked on the daemon (up to 620 s) must not hold the exit
    rt.shutdown_background();
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::StatusJson;

    fn state(net: &str) -> Reply {
        Reply {
            ok: Some(true),
            error: Some(None),
            status: Some(StatusJson {
                time: "2026-01-01 00:00:00".into(),
                net: net.into(),
                frame_commands: 0,
                paused: false,
                screen: (480, 800),
                app_id: "hello".into(),
                running: true,
                pending: None,
                locked: false,
                refresh: None,
                refreshes: vec![],
            }),
            shot: Some(None),
            ..Reply::default()
        }
    }

    fn error_of(a: &Attached) -> String {
        a.error
            .clone()
            .or_else(|| a.state.error.clone().flatten())
            .unwrap_or_default()
    }

    #[test]
    fn attaches_when_nothing_differs() {
        let a = attach_reply(state("replay"), false, Some(false), Some("replay"), false);
        assert_eq!(a.state.ok, Some(true));
        assert_eq!(a.state.error, Some(None));
        assert!(a.already_running && a.error.is_none());
        let a = attach_reply(state("replay"), false, None, None, false);
        assert_eq!(a.state.ok, Some(true));
    }

    #[test]
    fn refuses_another_mode_in_json_words() {
        let a = attach_reply(state("replay"), false, None, Some("record"), false);
        assert_eq!(a.state.ok, Some(false));
        assert_eq!(
            error_of(&a),
            "a daemon for this app is already running (net=replay, window=false) and cannot switch to \
             net=record; call emu_stop first, then emu_start again"
        );
        let a = attach_reply(state("record"), true, Some(false), Some("live"), true);
        assert!(error_of(&a).contains(
            "(net=record, window=true) and cannot switch to net=live, window=false, fresh=true;"
        ));
        let mut s = state("replay");
        s.status = None;
        let a = attach_reply(s, false, Some(true), None, false);
        assert!(error_of(&a).contains("(net=null, window=false) and cannot switch to window=true;"));
    }

    #[test]
    fn merged_keys_keep_python_order() {
        let small = |error| Reply {
            ok: Some(true),
            error,
            log_tail: Some(vec![]),
            ..Reply::default()
        };
        let msg = "a daemon for this app is already running (net=null, window=false) and cannot switch to \
                   net=live; call emu_stop first, then emu_start again";
        let a = attach_reply(small(Some(None)), false, None, Some("live"), false);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            format!(r#"{{"ok":false,"error":"{msg}","log_tail":[],"already_running":true}}"#),
        );
        let a = attach_reply(small(None), false, None, Some("live"), false);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            format!(r#"{{"ok":false,"log_tail":[],"already_running":true,"error":"{msg}"}}"#),
        );
        let a = attach_reply(small(Some(None)), false, None, None, false);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"{"ok":true,"error":null,"log_tail":[],"already_running":true}"#
        );
        let started = Started {
            state: small(Some(None)),
            pid: 7,
            port: 8,
        };
        assert_eq!(
            serde_json::to_string(&started).unwrap(),
            r#"{"ok":true,"error":null,"log_tail":[],"pid":7,"port":8}"#
        );
        let daemon = json!({"ok": true, "error": null, "texts": [], "log_tail": [], "errors": [],
            "warnings": [], "shot": null});
        let a = attach_reply(Reply::from_value(daemon), false, None, None, false);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"{"ok":true,"error":null,"texts":[],"log_tail":[],"errors":[],"warnings":[],"shot":null,"already_running":true}"#
        );
        let text = serde_json::to_string_pretty(&failure(NO_DAEMON)).unwrap();
        assert_eq!(
            text,
            "{\n  \"ok\": false,\n  \"error\": \"no emulator running; call emu_start first\"\n}"
        );
    }

    #[test]
    fn net_is_checked_and_shown_as_json() {
        assert_eq!(
            check_net(Some("wifi")).unwrap_err(),
            r#"net must be one of replay, record, live, got "wifi""#
        );
        assert!(check_net(Some("live")).is_ok() && check_net(None).is_ok());
    }

    #[test]
    fn tail_counts_characters() {
        assert_eq!(tail_chars("abc", 5), "abc");
        assert_eq!(tail_chars("aéц", 2), "éц");
    }
}
