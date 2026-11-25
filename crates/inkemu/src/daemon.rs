//! The daemon half of 0.1 `daemon.py`: `daemon.json`, `POST /cmd`, the serve loop and the HTTP client.
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{Map, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::command::execute;
use crate::{paths, Session};

pub const DEFAULT_PORT: u16 = 7480;
pub const HOST: &str = "127.0.0.1";
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(600);
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(620);

// ---- daemon.json -------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub port: u16,
    pub pid: u32,
    pub app: String,
    pub window: bool,
}

/// An integer as Python's `int()` takes it: a JSON number or an int-valued string.
fn lenient_int(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 only checks that the process exists.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn alive(_pid: u32) -> bool {
    false
}

/// `<root>/daemon.json` if its process is alive, else None.
pub fn read_info(root: &Path) -> Option<Info> {
    let text = std::fs::read_to_string(root.join("daemon.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let obj = v.as_object()?;
    let port = u16::try_from(lenient_int(obj.get("port")?)?).ok()?;
    let pid = u32::try_from(lenient_int(obj.get("pid")?)?).ok()?;
    if port == 0 || !alive(pid) {
        return None;
    }
    Some(Info {
        port,
        pid,
        app: obj
            .get("app")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        window: obj.get("window").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

pub fn write_info(root: &Path, port: u16, app: &str, window: bool) -> io::Result<()> {
    std::fs::create_dir_all(root)?;
    let text = format!(
        r#"{{"port": {port}, "pid": {}, "app": {}, "window": {window}}}"#,
        std::process::id(),
        json_str(app)
    );
    std::fs::write(root.join("daemon.json"), text)
}

/// Deletes `daemon.json` only if it is ours.
pub fn remove_info(root: &Path) {
    let path = root.join("daemon.json");
    let ours = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("pid").and_then(Value::as_u64))
        == Some(u64::from(std::process::id()));
    if ours {
        let _ = std::fs::remove_file(path);
    }
}

// ---- client ------------------------------------------------------------------------------------

/// Sends one command to the daemon on `port`; Err unless an inkemu daemon answers.
pub fn post_command(port: u16, cmd: &str, args: &Map<String, Value>) -> Result<Value, String> {
    let body = serde_json::json!({"cmd": cmd, "args": args}).to_string();
    let agent = ureq::AgentBuilder::new().timeout(CLIENT_TIMEOUT).build();
    let resp = agent
        .post(&format!("http://{HOST}:{port}/cmd"))
        .set("Content-Type", "application/json")
        .send_string(&body);
    let text = match resp {
        Ok(r) | Err(ureq::Error::Status(_, r)) => r.into_string(),
        Err(e) => return Err(format!("cannot reach the daemon on port {port}: {e}")),
    }
    .map_err(|e| format!("cannot reach the daemon on port {port}: {e}"))?;
    match serde_json::from_str::<Value>(&text) {
        Ok(v) if v.as_object().is_some_and(|o| o.contains_key("ok")) => Ok(v),
        _ => Err(format!(
            "the server on port {port} did not answer like an inkemu daemon"
        )),
    }
}

// ---- HTTP server -------------------------------------------------------------------------------

type Job = (String, Map<String, Value>, Sender<String>);

fn error_body(msg: &str) -> String {
    format!(r#"{{"ok": false, "error": {}}}"#, json_str(msg))
}

fn reply(req: Request, code: u16, body: String) {
    let ct = Header::from_bytes("Content-Type", "application/json; charset=utf-8")
        .expect("static header");
    let _ = req.respond(
        Response::from_string(body)
            .with_status_code(code)
            .with_header(ct),
    );
}

fn header<'a>(req: &'a Request, name: &str) -> Option<&'a str> {
    req.headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

/// The 403 reasons. A web page can POST to 127.0.0.1 (no-cors text/plain, DNS rebinding);
/// our clients send neither Origin nor a foreign Host.
fn forbidden(req: &Request, port: u16) -> Option<&'static str> {
    let host = header(req, "Host");
    if host != Some(&format!("127.0.0.1:{port}")) && host != Some(&format!("localhost:{port}")) {
        return Some("bad Host header");
    }
    if header(req, "Origin").is_some() {
        return Some("cross-origin requests are not allowed");
    }
    let ct = header(req, "Content-Type").unwrap_or("");
    if ct.split(';').next().unwrap_or("").trim().to_lowercase() != "application/json" {
        return Some("Content-Type must be application/json");
    }
    None
}

const BAD_SHAPE: &str = "cmd must be a string and args an object";

fn parse_request(raw: &[u8]) -> Result<(String, Map<String, Value>), String> {
    let v: Value = serde_json::from_slice(raw).map_err(|e| e.to_string())?;
    let obj = v.as_object().ok_or("the request must be a JSON object")?;
    let cmd = match obj.get("cmd") {
        None => return Err("missing 'cmd'".into()),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err(BAD_SHAPE.into()),
    };
    // Python's `req.get("args") or {}`: every falsy value means no arguments
    let args = match obj.get("args") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => Map::new(),
        Some(Value::Object(m)) => m.clone(),
        Some(Value::String(s)) if s.is_empty() => Map::new(),
        Some(Value::Array(a)) if a.is_empty() => Map::new(),
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => Map::new(),
        Some(_) => return Err(BAD_SHAPE.into()),
    };
    Ok((cmd, args))
}

fn handle(mut req: Request, port: u16, jobs: Sender<Job>) {
    if *req.method() != Method::Post {
        return reply(req, 405, error_body("POST /cmd only"));
    }
    if let Some(why) = forbidden(&req, port) {
        return reply(req, 403, error_body(why));
    }
    if req.url() != "/cmd" {
        return reply(req, 404, error_body("POST /cmd only"));
    }
    let mut raw = Vec::new();
    let parsed = req
        .as_reader()
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())
        .and_then(|_| parse_request(&raw));
    let (cmd, args) = match parsed {
        Ok(p) => p,
        Err(e) => return reply(req, 400, error_body(&format!("bad request: {e}"))),
    };
    let (tx, rx) = mpsc::channel();
    if jobs.send((cmd, args, tx)).is_err() {
        return reply(req, 200, error_body("the daemon is stopping"));
    }
    match rx.recv_timeout(REPLY_TIMEOUT) {
        Ok(body) => reply(req, 200, body),
        Err(RecvTimeoutError::Timeout) => {
            reply(req, 504, error_body("the emulator did not answer in time"))
        }
        // the daemon closed with this command still queued
        Err(RecvTimeoutError::Disconnected) => {
            reply(req, 200, error_body("the daemon is stopping"))
        }
    }
}

/// HTTP front of one `Session`. HTTP threads only enqueue; `pump` runs commands on the caller's thread.
pub struct Daemon {
    server: Arc<Server>,
    port: u16,
    jobs: mpsc::Receiver<Job>,
    closing: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
    stopped: bool,
}

impl Daemon {
    pub fn new(port: u16) -> Result<Daemon, String> {
        let server = Server::http((HOST, port)).map_err(|e| {
            let why = match e.downcast_ref::<io::Error>() {
                Some(io) => strerror(io),
                None => e.to_string(),
            };
            format!(
                "cannot listen on {HOST}:{port}: {why} (is another inkemu daemon running? try --port)"
            )
        })?;
        let server = Arc::new(server);
        let port = server
            .server_addr()
            .to_ip()
            .map_or(port, |addr| addr.port());
        let (tx, jobs) = mpsc::channel::<Job>();
        let closing = Arc::new(AtomicBool::new(false));
        let accept = {
            let (server, closing) = (server.clone(), closing.clone());
            thread::Builder::new()
                .name("inkemu-http".into())
                .spawn(move || {
                    while let Ok(req) = server.recv() {
                        if closing.load(Ordering::SeqCst) {
                            break;
                        }
                        let tx = tx.clone();
                        thread::spawn(move || handle(req, port, tx));
                    }
                })
                .map_err(|e| format!("cannot start the HTTP thread: {e}"))?
        };
        Ok(Daemon {
            server,
            port,
            jobs,
            closing,
            accept: Some(accept),
            stopped: false,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// Runs every queued command on this thread; returns how many ran.
    pub fn pump(&mut self, s: &mut Session) -> usize {
        let mut n = 0;
        while let Ok((cmd, args, tx)) = self.jobs.try_recv() {
            n += 1;
            let result = execute(s, &cmd, &args);
            if cmd == "stop" && result.ok {
                self.stopped = true;
            }
            let body = serde_json::to_string(&result)
                .unwrap_or_else(|e| error_body(&format!("cannot encode the reply: {e}")));
            let _ = tx.send(body);
        }
        n
    }

    /// Stops accepting requests and answers every still-queued command.
    pub fn close(mut self) {
        self.closing.store(true, Ordering::SeqCst);
        self.server.unblock();
        if let Some(t) = self.accept.take() {
            let _ = t.join();
        }
        while let Ok((_, _, tx)) = self.jobs.try_recv() {
            let _ = tx.send(error_body("the daemon is stopping"));
        }
    }
}

/// The OS text of an error, without Rust's " (os error N)" suffix.
fn strerror(e: &io::Error) -> String {
    let s = e.to_string();
    match (e.raw_os_error(), s.rfind(" (os error ")) {
        (Some(_), Some(i)) => s[..i].to_owned(),
        _ => s,
    }
}

// ---- serve -------------------------------------------------------------------------------------

static SIGINT: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_sigint(_: libc::c_int) {
    SIGINT.store(true, Ordering::SeqCst);
}

/// Sets `SIGINT` on Ctrl-C and puts the previous handler back on drop. Inside the Python process
/// Python's own handler never runs while Rust loops, so the flag is the only way out.
struct SigintGuard {
    #[cfg(unix)]
    previous: libc::sigaction,
}

impl SigintGuard {
    #[cfg(unix)]
    fn install() -> SigintGuard {
        SIGINT.store(false, Ordering::SeqCst);
        // SAFETY: plain sigaction calls on zeroed structs; the handler only stores an atomic.
        unsafe {
            let mut new: libc::sigaction = std::mem::zeroed();
            new.sa_sigaction = on_sigint as extern "C" fn(libc::c_int) as libc::sighandler_t;
            libc::sigemptyset(&mut new.sa_mask);
            let mut previous: libc::sigaction = std::mem::zeroed();
            libc::sigaction(libc::SIGINT, &new, &mut previous);
            SigintGuard { previous }
        }
    }

    #[cfg(not(unix))]
    fn install() -> SigintGuard {
        SigintGuard {}
    }
}

impl Drop for SigintGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: restores the action `install` replaced.
        unsafe {
            libc::sigaction(libc::SIGINT, &self.previous, std::ptr::null_mut());
        }
    }
}

/// Runs the headless daemon until `stop` or Ctrl-C.
pub fn serve(s: &mut Session, port: u16) -> Result<(), String> {
    let root = paths::state_root();
    let mut d = Daemon::new(port)?;
    let _guard = SigintGuard::install();
    if let Err(e) = write_info(&root, d.port(), &s.status().app_id, false) {
        d.close();
        return Err(format!("cannot write daemon.json: {e}"));
    }
    while !d.stopped() && !SIGINT.load(Ordering::SeqCst) {
        d.pump(s);
        thread::sleep(Duration::from_millis(20));
    }
    s.leave();
    remove_info(&root);
    d.close();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionOptions;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

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

    /// One HTTP request over a raw socket; `headers` replace the defaults when their name matches.
    fn raw(
        port: u16,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> (u16, Value) {
        let mut hs: Vec<(String, String)> = vec![
            ("Host".into(), format!("127.0.0.1:{port}")),
            ("Content-Type".into(), "application/json".into()),
        ];
        for (k, v) in headers {
            hs.retain(|(n, _)| !n.eq_ignore_ascii_case(k));
            if !v.is_empty() {
                hs.push((k.to_string(), v.to_string()));
            }
        }
        let mut req = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
        for (k, v) in &hs {
            req += &format!("{k}: {v}\r\n");
        }
        req += &format!("Content-Length: {}\r\n\r\n", body.len());
        let mut c = TcpStream::connect((HOST, port)).unwrap();
        c.write_all(req.as_bytes()).unwrap();
        c.write_all(body).unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        let lower = head.to_lowercase();
        assert!(
            lower.contains("content-type: application/json; charset=utf-8"),
            "{head}"
        );
        assert!(lower.contains("content-length: "), "{head}");
        (status, serde_json::from_str(body).unwrap())
    }

    fn post(port: u16, body: &str) -> (u16, Value) {
        raw(port, "POST", "/cmd", &[], body.as_bytes())
    }

    /// Runs `f` on a thread while this one pumps the daemon.
    fn pumping<T: Send + 'static>(
        d: &mut Daemon,
        s: &mut Session,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let h = thread::spawn(f);
        while !h.is_finished() {
            d.pump(s);
            thread::sleep(Duration::from_millis(5));
        }
        h.join().unwrap()
    }

    fn texts(v: &Value) -> String {
        v["texts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["text"].as_str().unwrap())
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn daemon_json_text_and_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("nested/root");
        write_info(&root, 1234, "hello", false).unwrap();
        let text = std::fs::read_to_string(root.join("daemon.json")).unwrap();
        assert_eq!(
            text,
            format!(
                r#"{{"port": 1234, "pid": {}, "app": "hello", "window": false}}"#,
                std::process::id()
            )
        );
        let info = read_info(&root).unwrap();
        assert_eq!(
            info,
            Info {
                port: 1234,
                pid: std::process::id(),
                app: "hello".into(),
                window: false
            }
        );
        remove_info(&root);
        assert!(read_info(&root).is_none());
    }

    #[test]
    fn read_info_rejects_missing_garbage_zero_port_and_dead_pid() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let me = std::process::id();
        assert!(read_info(root).is_none());
        let put = |t: &str| std::fs::write(root.join("daemon.json"), t).unwrap();
        for bad in [
            "garbage".to_string(),
            "[]".into(),
            r#"{"pid": 1}"#.into(),
            format!(r#"{{"port": 0, "pid": {me}}}"#),
            r#"{"port": 5, "pid": 0}"#.into(),
            r#"{"port": 5, "pid": -3}"#.into(),
            r#"{"port": 5, "pid": 4194999}"#.into(),
            format!(r#"{{"port": null, "pid": {me}}}"#),
        ] {
            put(&bad);
            assert!(read_info(root).is_none(), "{bad}");
        }
        put(&format!(r#"{{"port": "77", "pid": "{me}"}}"#));
        assert_eq!(
            read_info(root),
            Some(Info {
                port: 77,
                pid: me,
                app: String::new(),
                window: false
            })
        );
    }

    #[test]
    fn remove_info_keeps_a_foreign_file() {
        let tmp = tempfile::tempdir().unwrap();
        let other = std::process::id() + 1;
        std::fs::write(
            tmp.path().join("daemon.json"),
            format!(r#"{{"port": 5, "pid": {other}}}"#),
        )
        .unwrap();
        remove_info(tmp.path());
        assert!(tmp.path().join("daemon.json").exists());
        remove_info(&tmp.path().join("missing"));
    }

    #[test]
    fn tap_text_round_trip_over_http() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let mut d = Daemon::new(0).unwrap();
        let port = d.port();
        assert_ne!(port, 0);
        let (code, v) = pumping(&mut d, &mut s, move || {
            post(port, r#"{"cmd": "tap-text", "args": {"text": "Tap me"}}"#)
        });
        assert_eq!(code, 200);
        assert_eq!(v["ok"], json!(true));
        assert_eq!(v["status"]["paused"], json!(true));
        assert!(texts(&v).contains("Taps: 1"), "{v}");
        // `localhost` and falsy args are accepted
        let (code, v) = pumping(&mut d, &mut s, move || {
            raw(
                port,
                "POST",
                "/cmd",
                &[("Host", &format!("localhost:{port}"))],
                br#"{"cmd": "state", "args": []}"#,
            )
        });
        assert_eq!((code, &v["ok"]), (200, &json!(true)));
        // the client agrees
        let r = pumping(&mut d, &mut s, move || {
            post_command(port, "state", &Map::new())
        });
        assert_eq!(r.unwrap()["ok"], json!(true));
        d.close();
    }

    #[test]
    fn rejected_requests_never_reach_the_session() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let mut d = Daemon::new(0).unwrap();
        let port = d.port();
        let ok_body: &[u8] = br#"{"cmd": "state"}"#;
        #[allow(clippy::type_complexity)]
        let cases: Vec<(u16, &str, (&str, &str, Vec<(&str, String)>, &[u8]))> = vec![
            (405, "POST /cmd only", ("GET", "/cmd", vec![], b"")),
            (
                405,
                "POST /cmd only",
                (
                    "PUT",
                    "/cmd",
                    vec![
                        ("Origin", "http://evil.example".into()),
                        ("Content-Type", "text/plain".into()),
                    ],
                    b"",
                ),
            ),
            (405, "POST /cmd only", ("DELETE", "/other", vec![], b"")),
            (
                403,
                "bad Host header",
                (
                    "POST",
                    "/cmd",
                    vec![("Host", format!("evil.example:{port}"))],
                    ok_body,
                ),
            ),
            (
                403,
                "bad Host header",
                (
                    "POST",
                    "/cmd",
                    vec![("Host", "127.0.0.1:1".into())],
                    ok_body,
                ),
            ),
            (
                403,
                "cross-origin requests are not allowed",
                (
                    "POST",
                    "/cmd",
                    vec![("Origin", "http://evil.example".into())],
                    ok_body,
                ),
            ),
            (
                403,
                "Content-Type must be application/json",
                (
                    "POST",
                    "/cmd",
                    vec![("Content-Type", "text/plain".into())],
                    ok_body,
                ),
            ),
            (
                403,
                "Content-Type must be application/json",
                (
                    "POST",
                    "/cmd",
                    vec![("Content-Type", String::new())],
                    ok_body,
                ),
            ),
            (404, "POST /cmd only", ("POST", "/other", vec![], ok_body)),
            (404, "POST /cmd only", ("POST", "/cmd?x=1", vec![], ok_body)),
        ];
        for (code, msg, (method, path, hs, body)) in cases {
            let hs: Vec<(&str, &str)> = hs.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let (c, v) = raw(port, method, path, &hs, body);
            assert_eq!(
                (c, v["ok"].clone(), v["error"].as_str()),
                (code, json!(false), Some(msg)),
                "{method} {path}"
            );
        }
        // media type parameters and case are fine
        let h = thread::spawn(move || {
            raw(
                port,
                "POST",
                "/cmd",
                &[("Content-Type", "Application/JSON ; charset=utf-8")],
                b"[]",
            )
        });
        assert_eq!(h.join().unwrap().0, 400);
        for body in [
            "not json",
            r#"{"cmd": 1}"#,
            r#"{"cmd": "state", "args": [1]}"#,
            "{}",
            "[]",
            r#""state""#,
            "null",
            "",
        ] {
            let (c, v) = post(port, body);
            assert_eq!((c, v["ok"].clone()), (400, json!(false)), "{body}");
            assert!(
                v["error"].as_str().unwrap().starts_with("bad request: "),
                "{v}"
            );
        }
        assert_eq!(d.pump(&mut s), 0);
        assert!(!s.status().paused);
        d.close();
    }

    #[test]
    fn stop_sets_stopped() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = hello(tmp.path());
        let mut d = Daemon::new(0).unwrap();
        let port = d.port();
        assert!(!d.stopped());
        let (_, v) = pumping(&mut d, &mut s, move || post(port, r#"{"cmd": "nope"}"#));
        assert_eq!(v["ok"], json!(false));
        assert!(!d.stopped(), "a failed stop must not stop");
        let (code, v) = pumping(&mut d, &mut s, move || post(port, r#"{"cmd": "stop"}"#));
        assert_eq!((code, &v["ok"]), (200, &json!(true)));
        assert!(d.stopped());
        d.close();
    }

    #[test]
    fn close_answers_queued_commands() {
        let d = Daemon::new(0).unwrap();
        let port = d.port();
        let h = thread::spawn(move || post_command(port, "state", &Map::new()));
        thread::sleep(Duration::from_millis(400));
        d.close();
        let r = h.join().unwrap().unwrap();
        assert_eq!(r, json!({"ok": false, "error": "the daemon is stopping"}));
        let err = post_command(port, "state", &Map::new()).unwrap_err();
        assert!(
            err.starts_with(&format!("cannot reach the daemon on port {port}: ")),
            "{err}"
        );
    }

    #[test]
    fn a_port_in_use_is_reported() {
        let d = Daemon::new(0).unwrap();
        let err = Daemon::new(d.port()).err().unwrap();
        assert!(
            err.starts_with(&format!("cannot listen on 127.0.0.1:{}: ", d.port()))
                && err.ends_with(" (is another inkemu daemon running? try --port)")
                && !err.contains("os error"),
            "{err}"
        );
        d.close();
    }

    #[test]
    fn post_command_rejects_servers_that_are_not_inkemu() {
        for body in ["<html>hi</html>", r#"{"no": "ok"}"#, "[1]"] {
            let l = TcpListener::bind((HOST, 0)).unwrap();
            let port = l.local_addr().unwrap().port();
            let h = thread::spawn(move || {
                let (mut c, _) = l.accept().unwrap();
                let mut buf = [0u8; 4096];
                let _ = c.read(&mut buf);
                let _ = write!(
                    c,
                    "HTTP/1.1 500 Oops\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                // closing first can fail the client's socket setup on macOS
                let _ = c.read(&mut buf);
            });
            let err = post_command(port, "state", &Map::new()).unwrap_err();
            assert_eq!(
                err,
                format!("the server on port {port} did not answer like an inkemu daemon"),
                "{body}"
            );
            h.join().unwrap();
        }
    }

    #[test]
    fn post_command_reads_error_status_bodies() {
        let l = TcpListener::bind((HOST, 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        let h = thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = c.read(&mut buf);
            let body = r#"{"ok": false, "error": "x"}"#;
            let _ = write!(
                c,
                "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = c.read(&mut buf);
        });
        let v = post_command(port, "state", &Map::new()).unwrap();
        assert_eq!(v["error"], json!("x"));
        h.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn serve_stops_on_sigint_and_cleans_up() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        std::env::set_var("INKEMU_STATE_ROOT", &root);
        let mut s = hello(tmp.path());
        let watch = root.clone();
        let h = thread::spawn(move || {
            while read_info(&watch).is_none() {
                thread::sleep(Duration::from_millis(5));
            }
            let info = read_info(&watch).unwrap();
            assert_eq!(info.app, "hello");
            let r = post_command(info.port, "state", &Map::new());
            assert!(r.is_ok(), "{r:?}");
            // SAFETY: the handler is installed before daemon.json appears
            unsafe { libc::kill(libc::getpid(), libc::SIGINT) };
        });
        // a command is served while the loop runs
        serve(&mut s, 0).unwrap();
        h.join().unwrap();
        assert!(!root.join("daemon.json").exists());
    }
}
