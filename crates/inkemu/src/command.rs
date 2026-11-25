//! The command layer: the verb table, argument conversion, one command against a `Session`, the result shapes
//! and the `run --steps` loop.
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use inkcore::clock::parse_duration;
use inkcore::log::py_repr_str;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::steps::split_steps;
use crate::Session;

/// How many log lines a result carries as `log_tail`.
pub const LOG_TAIL: usize = 20;

const KEYS: [&str; 6] = ["up", "down", "left", "right", "ok", "back"];
const SWIPES: [&str; 4] = ["left", "right", "up", "down"];
const HOLD_MS: i64 = 800;
const SWIPE_AT: (i32, i32) = (240, 400);
const SWIPE_DISTANCE: i32 = 200;

#[derive(Clone, Copy)]
enum Conv {
    Int32,
    Int64,
    Str,
    Choice(&'static [&'static str]),
    YesNo,
    Duration,
}

struct Field {
    name: &'static str,
    conv: Conv,
    required: bool,
}

struct Verb {
    name: &'static str,
    usage: &'static str,
    fields: &'static [Field],
}

const fn req(name: &'static str, conv: Conv) -> Field {
    Field {
        name,
        conv,
        required: true,
    }
}

const fn opt(name: &'static str, conv: Conv) -> Field {
    Field {
        name,
        conv,
        required: false,
    }
}

const fn verb(name: &'static str, usage: &'static str, fields: &'static [Field]) -> Verb {
    Verb {
        name,
        usage,
        fields,
    }
}

const XY: [Field; 2] = [req("x", Conv::Int32), req("y", Conv::Int32)];
const XY_MS: [Field; 3] = [
    req("x", Conv::Int32),
    req("y", Conv::Int32),
    opt("ms", Conv::Int64),
];

/// In 0.1's dict order.
static VERBS: [Verb; 19] = [
    verb("tap", "tap X Y", &XY),
    verb("tap-text", "tap-text TEXT", &[req("text", Conv::Str)]),
    verb("double-tap", "double-tap X Y", &XY),
    verb(
        "swipe",
        "swipe DIR",
        &[req("direction", Conv::Choice(&SWIPES))],
    ),
    verb("hold", "hold X Y [MS]", &XY_MS),
    verb("key", "key KEY", &[req("key", Conv::Choice(&KEYS))]),
    verb("type", "type TEXT", &[req("text", Conv::Str)]),
    verb("answer", "answer yes|no", &[req("answer", Conv::YesNo)]),
    verb("wait", "wait DURATION", &[req("duration", Conv::Duration)]),
    verb("lock", "lock", &[]),
    verb("unlock", "unlock", &[]),
    verb("reload", "reload", &[]),
    verb("restart", "restart", &[]),
    verb("shot", "shot [PATH]", &[opt("path", Conv::Str)]),
    verb("texts", "texts", &[]),
    verb("log", "log", &[]),
    verb("state", "state", &[]),
    verb("resume", "resume", &[]),
    verb("stop", "stop", &[]),
];

/// (verb, usage) of every command.
pub fn verbs() -> impl Iterator<Item = (&'static str, &'static str)> {
    VERBS.iter().map(|v| (v.name, v.usage))
}

fn find(cmd: &str) -> Result<&'static Verb, String> {
    VERBS.iter().find(|v| v.name == cmd).ok_or_else(|| {
        let usages: Vec<_> = VERBS.iter().map(|v| v.usage).collect();
        format!(
            "unknown command {}; verbs: {}",
            py_repr_str(cmd),
            usages.join("; ")
        )
    })
}

// ---- converters ------------------------------------------------------------------------------------------

/// `'tap': x must be an integer, got "a"`: the offending value as JSON.
fn bad_value(cmd: &str, field: &str, what: &str, got: impl std::fmt::Display) -> String {
    format!("{}: {field} must be {what}, got {got}", py_repr_str(cmd))
}

const DURATION: &str = "milliseconds or a string like 350ms, 61s, 10m or 2h";

fn int_in_range(
    cmd: &str,
    f: &Field,
    n: i128,
    got: impl std::fmt::Display,
) -> Result<Value, String> {
    let fits = match f.conv {
        Conv::Int32 => i32::try_from(n).is_ok(),
        _ => i64::try_from(n).is_ok(),
    };
    if !fits {
        let bits = if matches!(f.conv, Conv::Int32) {
            "a 32-bit"
        } else {
            "a 64-bit"
        };
        return Err(bad_value(cmd, f.name, &format!("{bits} integer"), got));
    }
    Ok(Value::from(n as i64))
}

fn convert(cmd: &str, f: &Field, v: &Value) -> Result<Value, String> {
    let bad = |what: &str| bad_value(cmd, f.name, what, v);
    match f.conv {
        Conv::Int32 | Conv::Int64 => {
            let n = match (v.as_i64(), v.as_u64()) {
                (Some(i), _) => i128::from(i),
                (None, Some(u)) => i128::from(u),
                _ => return Err(bad("an integer")),
            };
            int_in_range(cmd, f, n, v)
        }
        Conv::Str => match v {
            Value::String(_) => Ok(v.clone()),
            _ => Err(bad("a string")),
        },
        Conv::Choice(options) => match v {
            Value::String(s) if options.contains(&s.as_str()) => Ok(v.clone()),
            _ => Err(bad(&format!("one of {}", options.join(", ")))),
        },
        Conv::YesNo => match v {
            Value::Bool(_) => Ok(v.clone()),
            Value::String(s) if s == "yes" || s == "no" => Ok(Value::Bool(s == "yes")),
            _ => Err(bad("yes or no")),
        },
        Conv::Duration => {
            let ms = match v {
                Value::String(s) => parse_duration(s).ok(),
                _ => v.as_i64().filter(|ms| *ms >= 0),
            };
            ms.map(Value::from).ok_or_else(|| bad(DURATION))
        }
    }
}

/// Validates and converts the args of one command.
pub fn normalize(cmd: &str, args: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    let verb = find(cmd)?;
    let mut extra: Vec<&str> = args
        .keys()
        .map(String::as_str)
        .filter(|k| !verb.fields.iter().any(|f| f.name == *k))
        .collect();
    if !extra.is_empty() {
        extra.sort_unstable();
        return Err(format!(
            "{}: unexpected argument(s) {}",
            py_repr_str(cmd),
            extra.join(", ")
        ));
    }
    let mut out = Map::new();
    for f in verb.fields {
        match args.get(f.name) {
            None | Some(Value::Null) => {
                if f.required {
                    return Err(format!(
                        "{}: missing argument {} (usage: {})",
                        py_repr_str(cmd),
                        py_repr_str(f.name),
                        verb.usage
                    ));
                }
            }
            Some(v) => {
                out.insert(f.name.into(), convert(cmd, f, v)?);
            }
        }
    }
    Ok(out)
}

/// `["tap", "1", "2"]` to `("tap", {x: 1, y: 2})`.
pub fn parse_words(words: &[String]) -> Result<(String, Map<String, Value>), String> {
    let Some((cmd, rest)) = words.split_first() else {
        return Err("empty command".into());
    };
    let verb = find(cmd)?;
    let lo = verb.fields.iter().filter(|f| f.required).count();
    let hi = verb.fields.len();
    if rest.len() < lo || rest.len() > hi {
        let want = if lo == hi {
            lo.to_string()
        } else {
            format!("{lo}-{hi}")
        };
        return Err(format!(
            "{} takes {want} argument(s), got {} (usage: {})",
            py_repr_str(cmd),
            rest.len(),
            verb.usage
        ));
    }
    let mut args = Map::new();
    for (f, w) in verb.fields.iter().zip(rest) {
        args.insert(f.name.into(), word_value(cmd, f, w)?);
    }
    Ok((cmd.clone(), normalize(cmd, &args)?))
}

/// The typed JSON value of a command-line word; a word that does not fit stays a string for `normalize` to refuse.
fn word_value(cmd: &str, f: &Field, w: &str) -> Result<Value, String> {
    match f.conv {
        Conv::Int32 | Conv::Int64 => {
            let digits = w.strip_prefix(['-', '+']).unwrap_or(w);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Ok(Value::String(w.into()));
            }
            // a word too long for i128 is out of range for every field anyway
            let n = w.parse::<i128>().unwrap_or(if w.starts_with('-') {
                i128::MIN
            } else {
                i128::MAX
            });
            int_in_range(cmd, f, n, w)
        }
        Conv::YesNo if w == "yes" || w == "no" => Ok(Value::Bool(w == "yes")),
        _ => Ok(Value::String(w.into())),
    }
}

// ---- results ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub font: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingJson {
    pub kind: String,
    pub title: String,
    pub text: String,
}

/// 0.1 `Emu.status()`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StatusJson {
    pub time: String,
    pub net: String,
    pub frame_commands: usize,
    pub paused: bool,
    pub screen: (u32, u32),
    pub app_id: String,
    pub running: bool,
    pub pending: Option<PendingJson>,
    pub locked: bool,
    pub refresh: Option<String>,
    pub refreshes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CmdResult {
    pub ok: bool,
    pub error: Option<String>,
    pub texts: Vec<Text>,
    pub log_tail: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub status: StatusJson,
    pub shot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<Vec<String>>,
}

/// An `error: null` has to stay different from no `error` at all, so it reads as `Some(None)`.
fn keep_null<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

/// What a client holds of a daemon answer: any field may be missing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(
        default,
        deserialize_with = "keep_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub error: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texts: Option<Vec<Text>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_tail: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusJson>,
    #[serde(
        default,
        deserialize_with = "keep_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub shot: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<Vec<String>>,
}

impl Reply {
    /// The daemon's answer, or just its `ok` and `error` if the rest does not fit.
    pub fn from_value(value: Value) -> Reply {
        serde_json::from_value(value.clone()).unwrap_or_else(|_| Reply {
            ok: value.get("ok").and_then(Value::as_bool),
            error: value.get("error").map(|e| e.as_str().map(str::to_owned)),
            ..Reply::default()
        })
    }
}

impl From<CmdResult> for Reply {
    fn from(r: CmdResult) -> Reply {
        Reply {
            ok: Some(r.ok),
            error: Some(r.error),
            texts: Some(r.texts),
            log_tail: Some(r.log_tail),
            errors: Some(r.errors),
            warnings: Some(r.warnings),
            status: Some(r.status),
            shot: Some(r.shot),
            log: r.log,
        }
    }
}

fn status_json(s: &Session) -> StatusJson {
    let st = s.status();
    StatusJson {
        time: st.time,
        net: st.net.into(),
        frame_commands: st.frame_commands,
        paused: st.paused,
        screen: st.screen,
        app_id: st.app_id,
        running: st.running,
        pending: st.pending.map(|p| PendingJson {
            kind: p.kind.into(),
            title: p.title,
            text: p.text,
        }),
        locked: st.locked,
        refresh: st.refresh.map(Into::into),
        refreshes: st.refreshes.into_iter().map(Into::into).collect(),
    }
}

/// What the screen shows as text, as 0.1 `Emu.texts()`.
pub fn texts(s: &Session) -> Vec<Text> {
    s.core()
        .texts()
        .into_iter()
        .map(|t| Text {
            text: t.text,
            x: t.x,
            y: t.y,
            w: t.w,
            h: t.h,
            font: t.font.into(),
        })
        .collect()
}

/// What the client prints for one command result.
pub fn format_result(cmd: &str, r: &Reply) -> String {
    match cmd {
        "shot" => {
            if let Some(Some(path)) = &r.shot {
                if !path.is_empty() {
                    return path.clone();
                }
            }
        }
        "texts" => {
            let lines: Vec<_> = r
                .texts
                .iter()
                .flatten()
                .map(|t| format!("{},{} {}", t.x, t.y, t.text))
                .collect();
            return lines.join("\n");
        }
        "log" => return r.log.clone().unwrap_or_default().join("\n"),
        _ => {}
    }
    #[derive(Serialize)]
    struct Printed<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        ok: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: &'a Option<Option<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        texts: &'a Option<Vec<Text>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        errors: &'a Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        warnings: &'a Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: &'a Option<StatusJson>,
        #[serde(skip_serializing_if = "Option::is_none")]
        log_tail: &'a Option<Vec<String>>,
    }
    let printed = Printed {
        ok: r.ok,
        error: &r.error,
        texts: &r.texts,
        errors: &r.errors,
        warnings: &r.warnings,
        status: &r.status,
        log_tail: &r.log_tail,
    };
    serde_json::to_string_pretty(&printed).unwrap_or_default()
}

// ---- screenshots -----------------------------------------------------------------------------------------

/// `<dir>/NNN.png` after the highest numbered PNG in `dir`.
pub fn next_shot(dir: &Path) -> PathBuf {
    let max = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension()? != "png" {
                return None;
            }
            let stem = p.file_stem()?.to_str()?;
            stem.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| stem.parse::<u64>().ok())
                .flatten()
        })
        .max()
        .unwrap_or(0);
    dir.join(format!("{:03}.png", max.saturating_add(1)))
}

/// An I/O error worded like Python's `OSError`.
fn os_error(e: &io::Error, path: &Path) -> String {
    match e.raw_os_error() {
        Some(code) => {
            let text = e.to_string();
            let msg = text
                .strip_suffix(&format!(" (os error {code})"))
                .unwrap_or(&text);
            format!(
                "[Errno {code}] {msg}: {}",
                py_repr_str(&path.to_string_lossy())
            )
        }
        None => e.to_string(),
    }
}

/// Writes the screen as a PNG whatever the extension: 8-bit grayscale for a gray frame, else 1-bit.
pub fn save_shot(s: &Session, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|e| os_error(&e, dir))?;
    }
    let (w, h, depth, data) = match s.core().frame_gray() {
        Some(g) => (g.w, g.h, png::BitDepth::Eight, g.gray_bytes()),
        None => {
            let f = s.frame(true);
            (f.w, f.h, png::BitDepth::One, f.packed())
        }
    };
    let file = File::create(path).map_err(|e| os_error(&e, path))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(depth);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&data).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}

// ---- execute ---------------------------------------------------------------------------------------------

fn int(args: &Map<String, Value>, name: &str) -> i64 {
    args.get(name).and_then(Value::as_i64).unwrap_or(0)
}

fn word<'a>(args: &'a Map<String, Value>, name: &str) -> &'a str {
    args.get(name).and_then(Value::as_str).unwrap_or("")
}

fn run(s: &mut Session, cmd: &str, a: &Map<String, Value>) -> Result<(), String> {
    let (x, y) = (int(a, "x") as i32, int(a, "y") as i32);
    match cmd {
        "tap" => s.tap(x, y)?,
        "tap-text" => s.tap_text(word(a, "text"))?,
        "double-tap" => s.double_tap(x, y)?,
        "swipe" => s.swipe(word(a, "direction"), SWIPE_AT, SWIPE_DISTANCE)?,
        "hold" => s.hold(x, y, a.get("ms").and_then(Value::as_i64).unwrap_or(HOLD_MS))?,
        "key" => s.key(word(a, "key"))?,
        "type" => s.type_text(word(a, "text"))?,
        "answer" => s.answer(a.get("answer").and_then(Value::as_bool).unwrap_or(false))?,
        "wait" => s.wait(int(a, "duration"))?,
        "lock" => s.lock(),
        "unlock" => s.unlock(),
        "reload" => s.reload(),
        "restart" => s.restart(),
        "resume" => s.resume(),
        _ => {}
    }
    Ok(())
}

/// The single command table: runs one command against `s` and describes the result.
pub fn execute(s: &mut Session, cmd: &str, args: &Map<String, Value>) -> CmdResult {
    let mut shot = None;
    let mut log = None;
    let outcome = normalize(cmd, args).and_then(|norm| {
        if cmd != "resume" {
            s.set_paused(true);
        }
        run(s, cmd, &norm)?;
        match cmd {
            "shot" => {
                let path = match norm.get("path").and_then(Value::as_str) {
                    Some(p) if !p.is_empty() => PathBuf::from(p),
                    _ => next_shot(&s.state_dir().join("shots")),
                };
                save_shot(s, &path)?;
                shot = Some(path.to_string_lossy().into_owned());
            }
            "log" => log = Some(s.core().log().to_vec()),
            _ => {}
        }
        Ok(())
    });
    let error = outcome.err();
    let log_tail = {
        let l = s.core().log();
        l[l.len().saturating_sub(LOG_TAIL)..].to_vec()
    };
    CmdResult {
        ok: error.is_none(),
        error,
        texts: texts(s),
        log_tail,
        errors: s.core().errors().to_vec(),
        warnings: s.core().warnings().to_vec(),
        status: status_json(s),
        shot,
        log,
    }
}

// ---- run --steps -----------------------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum RunError {
    /// Bad `--steps`: nothing ran (exit code 2).
    Usage(String),
    /// A step failed or a file could not be written (exit code 1).
    Step(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Usage(m) | RunError::Step(m) => f.write_str(m),
        }
    }
}

const PRINTED_IN_RUN: [&str; 4] = ["shot", "texts", "log", "state"];

/// Runs `;`-separated steps, printing shot paths and texts/log/state output in order, then writes `texts.json`.
/// Every step is parsed before any runs.
pub fn run_steps(
    s: &mut Session,
    steps: &str,
    out_dir: &Path,
    out: &mut dyn Write,
) -> Result<Vec<PathBuf>, RunError> {
    let mut plan = Vec::new();
    for words in split_steps(steps).map_err(RunError::Usage)? {
        plan.push(parse_words(&words).map_err(RunError::Usage)?);
    }
    if plan.is_empty() {
        return Err(RunError::Usage("no steps given".into()));
    }
    if plan.iter().any(|(cmd, _)| cmd == "stop") {
        return Err(RunError::Usage(
            "'stop' only makes sense against a running daemon".into(),
        ));
    }
    fs::create_dir_all(out_dir).map_err(|e| RunError::Step(os_error(&e, out_dir)))?;
    let mut written = Vec::new();
    let failed = run_plan(s, plan, out_dir, out, &mut written);
    let json = serde_json::to_string_pretty(&texts(s)).unwrap_or_default();
    let path = out_dir.join("texts.json");
    let saved = fs::write(&path, json).map_err(|e| RunError::Step(os_error(&e, &path)));
    match failed {
        Some(e) => Err(RunError::Step(e)),
        None => saved.map(|()| written),
    }
}

fn run_plan(
    s: &mut Session,
    plan: Vec<(String, Map<String, Value>)>,
    out_dir: &Path,
    out: &mut dyn Write,
    written: &mut Vec<PathBuf>,
) -> Option<String> {
    let mut numbered = 0;
    for (cmd, mut args) in plan {
        if cmd == "shot" && !args.contains_key("path") {
            numbered += 1;
            let path = out_dir.join(format!("{numbered:03}.png"));
            args.insert("path".into(), Value::String(path.to_string_lossy().into()));
        }
        let result = execute(s, &cmd, &args);
        if let Some(e) = result.error {
            return Some(e);
        }
        if let Some(p) = result.shot.as_deref().filter(|p| !p.is_empty()) {
            written.push(PathBuf::from(p));
        }
        if PRINTED_IN_RUN.contains(&cmd.as_str()) {
            let text = format_result(&cmd, &Reply::from(result));
            // a closed pipe must not stop the run: the files still get written
            let _ = writeln!(out, "{text}").and_then(|()| out.flush());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionOptions;
    use serde_json::json;

    fn args(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap_or_default()
    }

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    fn norm_err(cmd: &str, v: Value) -> String {
        normalize(cmd, &args(v)).unwrap_err()
    }

    #[test]
    fn verbs_keep_the_order_and_usages_of_0_1() {
        let got: Vec<_> = verbs().map(|(_, usage)| usage).collect();
        assert_eq!(
            got.join("; "),
            "tap X Y; tap-text TEXT; double-tap X Y; swipe DIR; hold X Y [MS]; key KEY; type TEXT; \
             answer yes|no; wait DURATION; lock; unlock; reload; restart; shot [PATH]; texts; log; \
             state; resume; stop"
        );
        assert_eq!(verbs().count(), 19);
        assert_eq!(verbs().next().map(|v| v.0), Some("tap"));
    }

    #[test]
    fn unknown_and_arity_messages() {
        let e = normalize("jump", &Map::new()).unwrap_err();
        assert!(e.starts_with("unknown command 'jump'; verbs: tap X Y; tap-text TEXT;"));
        assert!(e.ends_with("; resume; stop"));
        assert_eq!(parse_words(&[]).unwrap_err(), "empty command");
        assert_eq!(
            parse_words(&words("tap 1")).unwrap_err(),
            "'tap' takes 2 argument(s), got 1 (usage: tap X Y)"
        );
        assert_eq!(
            parse_words(&words("hold 1")).unwrap_err(),
            "'hold' takes 2-3 argument(s), got 1 (usage: hold X Y [MS])"
        );
        assert_eq!(
            parse_words(&words("lock now")).unwrap_err(),
            "'lock' takes 0 argument(s), got 1 (usage: lock)"
        );
        assert_eq!(
            norm_err("tap", json!({"x": 1, "b": 2, "a": 3})),
            "'tap': unexpected argument(s) a, b"
        );
        assert_eq!(
            norm_err("tap", json!({"x": 1})),
            "'tap': missing argument 'y' (usage: tap X Y)"
        );
        assert_eq!(
            norm_err("tap", json!({"x": 1, "y": null})),
            "'tap': missing argument 'y' (usage: tap X Y)"
        );
    }

    #[test]
    fn parse_words_converts() {
        let (cmd, a) = parse_words(&words("hold 1 -2 +300")).unwrap();
        assert_eq!(cmd, "hold");
        assert_eq!(Value::Object(a), json!({"x": 1, "y": -2, "ms": 300}));
        let (_, a) = parse_words(&words("answer yes")).unwrap();
        assert_eq!(a["answer"], json!(true));
        let (_, a) = parse_words(&words("shot")).unwrap();
        assert!(a.is_empty());
        let (_, a) = parse_words(&["tap-text".into(), "Tap me".into()]).unwrap();
        assert_eq!(a["text"], json!("Tap me"));
    }

    #[test]
    fn parse_words_types_words_by_field() {
        let (_, a) = parse_words(&words("type 12")).unwrap();
        assert_eq!(a["text"], json!("12"));
        let (_, a) = parse_words(&words("shot 2024")).unwrap();
        assert_eq!(a["path"], json!("2024"));
        let (_, a) = parse_words(&words("answer no")).unwrap();
        assert_eq!(a["answer"], json!(false));
        let (_, a) = parse_words(&words("wait 350")).unwrap();
        assert_eq!(a["duration"], json!(350));
        let (_, a) = parse_words(&words("wait 1.5s")).unwrap();
        assert_eq!(a["duration"], json!(1500));
        let err = |line: &str| parse_words(&words(line)).unwrap_err();
        assert_eq!(err("tap a 2"), r#"'tap': x must be an integer, got "a""#);
        assert_eq!(
            err("tap 1_0 2"),
            r#"'tap': x must be an integer, got "1_0""#
        );
        assert_eq!(
            err("tap 1.5 2"),
            r#"'tap': x must be an integer, got "1.5""#
        );
        assert_eq!(err("tap - 2"), r#"'tap': x must be an integer, got "-""#);
        assert_eq!(
            err("tap 2147483648 2"),
            "'tap': x must be a 32-bit integer, got 2147483648"
        );
        assert_eq!(
            err("hold 1 2 999999999999999999999999999999999999999999"),
            "'hold': ms must be a 64-bit integer, got 999999999999999999999999999999999999999999"
        );
        assert_eq!(
            err("answer maybe"),
            r#"'answer': answer must be yes or no, got "maybe""#
        );
        assert_eq!(
            err("wait soon"),
            r#"'wait': duration must be milliseconds or a string like 350ms, 61s, 10m or 2h, got "soon""#
        );
    }

    #[test]
    fn int_converter_takes_json_integers_only() {
        let ok =
            |v: Value| normalize("tap", &args(json!({"x": v, "y": 0}))).map(|m| m["x"].clone());
        assert_eq!(ok(json!(5)), Ok(json!(5)));
        assert_eq!(ok(json!(-2147483648_i64)), Ok(json!(-2147483648_i64)));
        let bad = |v: Value| ok(v).unwrap_err();
        let not_int = |got: &str| format!("'tap': x must be an integer, got {got}");
        assert_eq!(bad(json!(true)), not_int("true"));
        assert_eq!(bad(json!(5.0)), not_int("5.0"));
        assert_eq!(bad(json!(-5.9)), not_int("-5.9"));
        assert_eq!(bad(json!("12")), not_int(r#""12""#));
        assert_eq!(bad(json!("a'b")), not_int(r#""a'b""#));
        assert_eq!(bad(json!([1, 2])), not_int("[1,2]"));
        assert_eq!(bad(json!({"a": 1})), not_int(r#"{"a":1}"#));
        assert_eq!(
            bad(json!(2147483648_i64)),
            "'tap': x must be a 32-bit integer, got 2147483648"
        );
        let ms = normalize("hold", &args(json!({"x": 1, "y": 2, "ms": 4294967296_i64}))).unwrap();
        assert_eq!(ms["ms"], json!(4294967296_i64));
        assert_eq!(
            norm_err(
                "hold",
                json!({"x": 1, "y": 2, "ms": 9223372036854775808_u64})
            ),
            "'hold': ms must be a 64-bit integer, got 9223372036854775808"
        );
    }

    #[test]
    fn other_converters_take_their_json_type_only() {
        let one = |cmd: &str, field: &str, v: Value| normalize(cmd, &args(json!({ field: v })));
        assert_eq!(
            one("swipe", "direction", json!("diag")).unwrap_err(),
            r#"'swipe': direction must be one of left, right, up, down, got "diag""#
        );
        assert_eq!(
            one("key", "key", json!(3)).unwrap_err(),
            "'key': key must be one of up, down, left, right, ok, back, got 3"
        );
        assert!(one("key", "key", json!("ok")).is_ok());
        let answer = |v: Value| one("answer", "answer", v);
        assert_eq!(answer(json!("yes")).unwrap()["answer"], json!(true));
        assert_eq!(answer(json!("no")).unwrap()["answer"], json!(false));
        assert_eq!(answer(json!(false)).unwrap()["answer"], json!(false));
        assert_eq!(
            answer(json!(1)).unwrap_err(),
            "'answer': answer must be yes or no, got 1"
        );
        let wait = |v: Value| one("wait", "duration", v);
        assert_eq!(wait(json!("61s")).unwrap()["duration"], json!(61000));
        assert_eq!(wait(json!("350")).unwrap()["duration"], json!(350));
        assert_eq!(wait(json!(350)).unwrap()["duration"], json!(350));
        assert_eq!(wait(json!(0)).unwrap()["duration"], json!(0));
        for bad in [
            json!("soon"),
            json!(-1),
            json!(1.5),
            json!(true),
            json!("-5s"),
        ] {
            assert_eq!(
                wait(bad.clone()).unwrap_err(),
                format!("'wait': duration must be {DURATION}, got {bad}")
            );
        }
        let text = |v: Value| one("type", "text", v);
        assert_eq!(text(json!("5")).unwrap()["text"], json!("5"));
        for bad in [json!(5), json!(true), json!([1, 2]), json!(1.5)] {
            assert_eq!(
                text(bad.clone()).unwrap_err(),
                format!("'type': text must be a string, got {bad}")
            );
        }
        assert_eq!(
            one("shot", "path", json!(7)).unwrap_err(),
            "'shot': path must be a string, got 7"
        );
    }

    fn reply_of(v: Value) -> Reply {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn format_result_shot_texts_log() {
        let r = reply_of(json!({"shot": "/tmp/a.png", "ok": true}));
        assert_eq!(format_result("shot", &r), "/tmp/a.png");
        let r = reply_of(json!({"texts": [
            {"text": "Hello", "x": 1, "y": 2, "w": 3, "h": 4, "font": "small"},
            {"text": "Мир", "x": 5, "y": 6, "w": 3, "h": 4, "font": "large"}]}));
        assert_eq!(format_result("texts", &r), "1,2 Hello\n5,6 Мир");
        assert_eq!(format_result("texts", &reply_of(json!({"texts": []}))), "");
        assert_eq!(
            format_result("log", &reply_of(json!({"log": ["a", "b"]}))),
            "a\nb"
        );
        assert_eq!(format_result("log", &reply_of(json!({"ok": true}))), "");
        assert_eq!(format_result("log", &reply_of(json!({"log": []}))), "");
    }

    #[test]
    fn format_result_json_filters_and_orders_keys() {
        let r = reply_of(json!({
            "log_tail": ["l"], "shot": null, "error": null, "ok": false,
            "log": ["x"], "errors": [], "warnings": ["w"], "texts": []
        }));
        let want =
            "{\n  \"ok\": false,\n  \"error\": null,\n  \"texts\": [],\n  \"errors\": [],\n  \
                    \"warnings\": [\n    \"w\"\n  ],\n  \"log_tail\": [\n    \"l\"\n  ]\n}";
        assert_eq!(format_result("state", &r), want);
        // a shot without a path falls through to the JSON
        let r = reply_of(json!({"ok": true, "shot": null}));
        assert_eq!(format_result("shot", &r), "{\n  \"ok\": true\n}");
        // no error key at all stays missing
        assert_eq!(format_result("state", &reply_of(json!({}))), "{}");
    }

    #[test]
    fn reply_round_trip_keeps_null_error_and_key_order() {
        let text = r#"{"ok":true,"error":null,"texts":[],"log_tail":[],"errors":[],"warnings":[],"shot":null}"#;
        let r: Reply = serde_json::from_str(text).unwrap();
        assert_eq!(r.error, Some(None));
        assert_eq!(r.shot, Some(None));
        assert_eq!(serde_json::to_string(&r).unwrap(), text);
    }

    #[test]
    fn reply_from_value_keeps_a_null_shot_in_place_and_falls_back_to_ok_and_error() {
        let daemon = json!({"ok": true, "error": null, "texts": [], "log_tail": [], "errors": [],
            "warnings": [], "shot": null});
        let r = Reply::from_value(daemon);
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"ok":true,"error":null,"texts":[],"log_tail":[],"errors":[],"warnings":[],"shot":null}"#
        );
        let r = Reply::from_value(json!({"ok": false, "error": "boom", "texts": "not a list"}));
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"ok":false,"error":"boom"}"#
        );
    }

    #[test]
    fn shot_path_is_kept_as_given_and_always_png() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let dir = tmp.path().to_str().unwrap();
        for given in [
            format!("{dir}//sub/./own.png"),
            format!("{dir}/noext"),
            format!("{dir}/pic.jpg"),
        ] {
            let r = execute(&mut s, "shot", &args(json!({ "path": given })));
            assert_eq!(r.shot.as_deref(), Some(given.as_str()), "{:?}", r.error);
            let bytes = fs::read(&given).unwrap();
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{given}");
        }
    }

    #[test]
    fn next_shot_counts_from_the_highest_png() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        assert_eq!(next_shot(&d.join("missing")), d.join("missing/001.png"));
        assert_eq!(next_shot(d), d.join("001.png"));
        for n in ["001.png", "007.png", "x.png", "9.txt", "12a.png"] {
            fs::write(d.join(n), b"").unwrap();
        }
        assert_eq!(next_shot(d), d.join("008.png"));
        fs::write(d.join("1000.png"), b"").unwrap();
        assert_eq!(next_shot(d), d.join("1001.png"));
    }

    // ---- against a session -----------------------------------------------------------------------

    fn hello(tmp: &Path) -> Session {
        let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello");
        let opts = SessionOptions {
            state_dir: Some(tmp.join("state")),
            fixtures_dir: Some(tmp.join("fixtures")),
            secrets_file: Some(tmp.join("secrets.json")),
            start_time: Some("2026-11-20T09:00:00".into()),
            ..SessionOptions::default()
        };
        Session::open(&app, opts).unwrap()
    }

    fn exec(s: &mut Session, line: &str) -> CmdResult {
        let (cmd, a) = parse_words(&split_steps(line).unwrap()[0]).unwrap();
        execute(s, &cmd, &a)
    }

    fn has_text(r: &CmdResult, needle: &str) -> bool {
        r.texts.iter().any(|t| t.text.contains(needle))
    }

    #[test]
    fn execute_tap_text_round_trip_and_pause() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let r = exec(&mut s, "state");
        assert!(r.ok && r.error.is_none(), "{:?}", r.error);
        assert!(r.status.paused, "every command but resume pauses");
        assert!(has_text(&r, "Tap me") && has_text(&r, "Taps: 0"));
        let r = exec(&mut s, "tap-text 'Tap me'");
        assert!(r.ok, "{:?}", r.error);
        assert!(r.status.paused);
        assert!(has_text(&r, "Taps: 1"), "{:?}", r.texts);
        let r = exec(&mut s, "resume");
        assert!(r.ok && !r.status.paused);
        assert_eq!(r.status.app_id, "hello");
        assert_eq!(r.status.screen, (480, 800));
        assert!(r.log.is_none());
        assert!(r.log_tail.len() <= LOG_TAIL);
    }

    #[test]
    fn execute_reports_errors_without_pausing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let r = execute(&mut s, "jump", &Map::new());
        assert!(!r.ok && r.shot.is_none());
        assert!(r.error.unwrap().starts_with("unknown command 'jump'"));
        assert!(!r.status.paused);
        let r = execute(&mut s, "tap", &args(json!({"x": "abc", "y": 1})));
        assert_eq!(
            r.error.as_deref(),
            Some(r#"'tap': x must be an integer, got "abc""#)
        );
        assert!(!r.status.paused);
        let r = execute(&mut s, "tap-text", &Map::new());
        assert_eq!(
            r.error.as_deref(),
            Some("'tap-text': missing argument 'text' (usage: tap-text TEXT)")
        );
        let r = exec(&mut s, "tap-text 'No such text anywhere'");
        assert!(!r.ok);
        assert!(r.error.is_some_and(|e| !e.is_empty()));
        assert!(r.status.paused);
    }

    #[test]
    fn execute_shot_numbering_and_explicit_path() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let r = exec(&mut s, "shot");
        let first = tmp.path().join("state/shots/001.png");
        assert_eq!(r.shot.as_deref(), Some(first.to_str().unwrap()));
        assert!(first.is_file());
        let r = exec(&mut s, "shot");
        assert_eq!(
            r.shot.as_deref(),
            Some(tmp.path().join("state/shots/002.png").to_str().unwrap())
        );
        let own = tmp.path().join("deep/dir/own.png");
        let r = execute(
            &mut s,
            "shot",
            &args(json!({"path": own.to_str().unwrap()})),
        );
        assert_eq!(r.shot.as_deref(), own.to_str());
        let reader = png::Decoder::new(std::io::BufReader::new(File::open(&own).unwrap()))
            .read_info()
            .unwrap();
        let info = reader.info();
        assert_eq!((info.width, info.height), (480, 800));
        assert_eq!(info.color_type, png::ColorType::Grayscale);
        let r = exec(&mut s, "log");
        assert!(r.log.is_some());
    }

    #[test]
    fn run_steps_prints_in_order_and_writes_texts_json() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let out_dir = tmp.path().join("out");
        let mut out = Vec::new();
        let steps =
            "texts; shot; tap-text 'Tap me'; shot; shot /tmp/ignored-inkemu-test.png; state";
        let steps = steps.replace(
            "/tmp/ignored-inkemu-test.png",
            out_dir.join("named.png").to_str().unwrap(),
        );
        let written = run_steps(&mut s, &steps, &out_dir, &mut out).unwrap();
        assert_eq!(
            written,
            vec![
                out_dir.join("001.png"),
                out_dir.join("002.png"),
                out_dir.join("named.png")
            ]
        );
        let text = String::from_utf8(out).unwrap();
        let at = |needle: &str| text.find(needle).unwrap_or(usize::MAX);
        let shots: Vec<_> = ["001.png", "002.png", "named.png"].map(at).into();
        assert!(text.starts_with("155,40 Hello, X4 Pro!\n"), "{text}");
        assert!(at("Taps: 0") < shots[0] && shots[0] < shots[1] && shots[1] < shots[2]);
        assert!(shots[2] < at("{\n  \"ok\": true"));
        assert!(text.contains(out_dir.join("001.png").to_str().unwrap()));
        let saved: Value =
            serde_json::from_slice(&fs::read(out_dir.join("texts.json")).unwrap()).unwrap();
        assert!(saved.as_array().is_some_and(|a| !a.is_empty()));
    }

    #[test]
    fn run_steps_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let out_dir = tmp.path().join("out");
        let mut out = Vec::new();
        let mut run = |steps: &str, s: &mut Session| run_steps(s, steps, &out_dir, &mut out);
        assert_eq!(
            run("  ", &mut s),
            Err(RunError::Usage("no steps given".into()))
        );
        assert_eq!(
            run("tap 1 2; stop", &mut s),
            Err(RunError::Usage(
                "'stop' only makes sense against a running daemon".into()
            ))
        );
        assert_eq!(
            run("tap 1", &mut s),
            Err(RunError::Usage(
                "'tap' takes 2 argument(s), got 1 (usage: tap X Y)".into()
            ))
        );
        assert!(
            matches!(run("'abc", &mut s), Err(RunError::Usage(m)) if m.starts_with("cannot parse --steps"))
        );
        assert!(!out_dir.exists(), "a usage error writes nothing");
        // a failed step still leaves texts.json, later steps do not run
        let r = run("tap-text 'No such text anywhere'; shot", &mut s);
        assert!(matches!(r, Err(RunError::Step(_))), "{r:?}");
        assert!(out_dir.join("texts.json").is_file());
        assert!(!out_dir.join("001.png").exists());
    }

    #[test]
    fn run_steps_prints_the_out_dir_as_given() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let dir = tmp.path().to_str().unwrap();
        let mut out = Vec::new();
        let messy = PathBuf::from(format!("{dir}/./out/"));
        let written = run_steps(&mut s, "shot", &messy, &mut out).unwrap();
        let want = format!("{dir}/./out/001.png");
        let written: Vec<_> = written.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(written, [want.as_str()]);
        assert_eq!(String::from_utf8(out).unwrap(), format!("{want}\n"));
    }

    #[test]
    fn run_steps_out_dir_that_is_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let file = tmp.path().join("file");
        fs::write(&file, b"").unwrap();
        let r = run_steps(&mut s, "state", &file, &mut Vec::new());
        assert!(matches!(r, Err(RunError::Step(m)) if m.starts_with("[Errno ")));
    }
}
