//! `ctx.net` (0.1 `net.install`): `replay` is answered here from fixtures. `record` and `live` requests go to the
//! host's [`Fetcher`] at request time, as 0.1's synchronous fetch; a host without one (the web page) takes them
//! from an outbox and hands the answer back through [`Core::deliver`].
//!
//! [`Core::deliver`]: crate::Core::deliver
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Function, IntoLuaMulti, Lua, Table, Value};

use super::object::{method, new_object};
use super::{arg, check_arg, int_value, nil, round_ms, soft_fail};
use crate::clock::TimerId;
use crate::config::NetMode;
use crate::emu::{with, Host, Shared, State};
use crate::fixtures::{FixtureSet, Redactor};
use crate::log::widen;

pub const DEFAULT_DELAY_MS: i64 = 300;
const BLOCKED: &str =
    "recording refused: fix secrets.json first, otherwise a secret could be saved in a fixture";

/// A `record`/`live` request for the host to perform. The URL, headers and body are as the app wrote them,
/// `{{name}}` placeholders included.
#[derive(Clone, Debug, PartialEq)]
pub struct OutRequest {
    pub id: u64,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// 0: the host's default.
    pub timeout_ms: i64,
}

/// What a request got: a response, or an error code the app sees as `err` (`timeout`, `no_network`, ...).
#[derive(Clone, Debug, PartialEq)]
pub enum NetResult {
    Response {
        status: i64,
        /// Lowercase names.
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        delay_ms: i64,
    },
    Error(String),
}

/// A line the fetcher hands back for the core's log, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum NetNote {
    Log(String),
    Warn(String),
}

/// What a fetcher got for one request, and what to log about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Fetched {
    pub result: NetResult,
    pub notes: Vec<NetNote>,
}

/// Performs a `record`/`live` request synchronously. It must not call back into the core.
pub type Fetcher = Box<dyn FnMut(&OutRequest) -> Fetched>;

/// A request the app is waiting for.
struct Job {
    f: Function,
    /// Clock ms when the app asked.
    t0: i64,
    active: Cell<bool>,
    timer: Cell<Option<TimerId>>,
}

#[derive(Default)]
pub(crate) struct NetState {
    outbox: Vec<OutRequest>,
    /// Outgoing requests by id until the host answers them.
    waiting: HashMap<u64, Rc<Job>>,
    next_id: u64,
    /// Requests the app still waits for (0.1 `net_inflight`), for the lock screen.
    pub(crate) inflight: u32,
    pub(crate) fixtures: Rc<FixtureSet>,
    pub(crate) redactor: Rc<Redactor>,
    pub(crate) fetcher: Option<Fetcher>,
}

impl NetState {
    pub(crate) fn new(fixtures: FixtureSet, redactor: Redactor) -> NetState {
        NetState {
            fixtures: Rc::new(fixtures),
            redactor: Rc::new(redactor),
            ..NetState::default()
        }
    }

    /// The app stopped: answers still on their way are dropped (0.1 cancelled their timers).
    pub(crate) fn stop(&mut self) {
        self.waiting.clear();
    }

    pub(crate) fn take_outgoing(&mut self) -> Vec<OutRequest> {
        std::mem::take(&mut self.outbox)
    }
}

fn done(s: &mut State, job: &Job) {
    if job.active.replace(false) {
        s.net.inflight = s.net.inflight.saturating_sub(1);
    }
}

/// Calls the app's callback with the response and counts the request as finished.
fn fire(h: &Host, job: &Job, resp: &NetResult) {
    job.timer.set(None);
    if let Ok(t) = response_table(h.lua, resp) {
        h.protected(&job.f, (h.ctx(), t), "ctx.net:request callback");
    }
    with(h.st, |s| done(s, job));
}

fn schedule(s: &mut State, job: Rc<Job>, resp: NetResult, delay_ms: i64) {
    let resp = Rc::new(resp);
    let j = job.clone();
    let id = s.add_timer(delay_ms, Rc::new(move |h: &Host| fire(h, &j, &resp)), 0);
    job.timer.set(Some(id));
}

fn delay_of(resp: &NetResult) -> i64 {
    match resp {
        NetResult::Response { delay_ms, .. } => *delay_ms,
        NetResult::Error(_) => DEFAULT_DELAY_MS,
    }
}

/// The host's answer to an outgoing request: the callback fires at request time + delay, or at once when that
/// moment has already passed. Unknown ids (cancelled, or from before a restart) are ignored.
pub(crate) fn deliver(h: &Host, id: u64, resp: NetResult) {
    let Some((job, now)) = with(h.st, |s| {
        let job = s.net.waiting.remove(&id)?;
        Some((job, s.sched.clock.ms))
    }) else {
        return;
    };
    let remaining = job.t0.saturating_add(delay_of(&resp)).saturating_sub(now);
    if remaining <= 0 {
        fire(h, &job, &resp);
        h.settle();
    } else {
        with(h.st, |s| schedule(s, job, resp, remaining));
    }
}

fn response_table(lua: &Lua, resp: &NetResult) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let headers = lua.create_table()?;
    match resp {
        NetResult::Response {
            status,
            headers: h,
            body,
            ..
        } => {
            t.raw_set("status", int_value(*status))?;
            t.raw_set("code", int_value(*status))?;
            for (k, v) in h {
                headers.raw_set(k.as_str(), v.as_str())?;
            }
            t.raw_set("body", lua.create_string(body)?)?;
            t.raw_set("bytes", int_value(body.len() as i64))?;
        }
        NetResult::Error(err) => {
            t.raw_set("status", 0)?;
            t.raw_set("code", 0)?;
            t.raw_set("err", err.as_str())?;
            t.raw_set("error", err.as_str())?;
        }
    }
    t.raw_set("headers", headers)?;
    Ok(t)
}

/// 0.1 `_URL.match`: `https?://` and a host character, any case.
fn url_ok(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    let rest = if lower.starts_with("http://") {
        &url[7..]
    } else if lower.starts_with("https://") {
        &url[8..]
    } else {
        return false;
    };
    rest.chars().next().is_some_and(|c| {
        !(c == '/' || c == '?' || c == '#' || c.is_whitespace() || ('\x1c'..='\x1f').contains(&c))
    })
}

/// Python `f"{x:g}"`.
fn py_g(x: f64) -> String {
    if !x.is_finite() {
        return if x.is_nan() {
            "nan"
        } else if x > 0.0 {
            "inf"
        } else {
            "-inf"
        }
        .into();
    }
    let sci = format!("{x:.5e}");
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let strip = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    if (-4..6).contains(&exp) {
        strip(&format!("{x:.*}", (5 - exp) as usize))
    } else {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", strip(mant), exp.abs())
    }
}

fn header_pairs(lua: &Lua, v: &Value) -> mlua::Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let Value::Table(t) = v else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for pair in t.pairs::<Value, Value>() {
        let (k, v) = pair?;
        let Value::String(k) = k else { continue };
        let v = match v {
            Value::String(s) => s.as_bytes().to_vec(),
            n @ (Value::Integer(_) | Value::Number(_)) => match lua.coerce_string(n)? {
                Some(s) => s.as_bytes().to_vec(),
                None => continue,
            },
            _ => continue,
        };
        out.push((k.as_bytes().to_vec(), v));
    }
    Ok(out)
}

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    with(st, |s| {
        for p in s.cfg.secret_problems.clone() {
            s.warn(&format!("net: {p}"));
        }
        if !s.cfg.secret_problems.is_empty() && s.cfg.net_mode == NetMode::Record {
            let msg = format!("net: {BLOCKED}");
            s.log.errors.push(msg.clone());
            s.log_line(&format!("ERROR {msg}"));
        }
    });
    let request = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            check_arg("request", 1, arg(&a, 0), "table")?;
            check_arg("request", 2, arg(&a, 1), "function")?;
            let (Value::Table(o), Value::Function(f)) = (arg(&a, 0).clone(), arg(&a, 1).clone())
            else {
                unreachable!()
            };
            // read like 0.1's Lua shim: plain indexing, strings only
            let text = |k: &str| -> mlua::Result<Option<Vec<u8>>> {
                Ok(match o.get::<Value>(k)? {
                    Value::String(s) => Some(s.as_bytes().to_vec()),
                    _ => None,
                })
            };
            let (url, method_b, body) = (text("url")?, text("method")?, text("body")?);
            let timeout = o.get::<Value>("timeout_ms")?;
            let headers = header_pairs(lua, &o.get::<Value>("headers")?)?;
            let Some(url) = url else {
                return Err(mlua::Error::runtime("bad extra argument (string expected)"));
            };
            let method_b = method_b.unwrap_or_else(|| b"GET".to_vec());
            let (Ok(url), true) = (String::from_utf8(url), method_b.is_ascii()) else {
                return soft_fail(lua, "invalid_argument");
            };
            let verb = String::from_utf8(method_b).unwrap_or_default();
            if !url_ok(&url) || verb.is_empty() {
                return soft_fail(lua, "invalid_argument");
            }
            let mut hdrs = Vec::new();
            for (k, v) in headers {
                match (String::from_utf8(k), String::from_utf8(v)) {
                    (Ok(k), Ok(v)) => hdrs.push((k, v)),
                    _ => with(&st, |s| {
                        s.warn("net: a header that is not valid UTF-8 was dropped")
                    }),
                }
            }
            let timeout_ms = match timeout {
                v @ (Value::Integer(_) | Value::Number(_)) => round_ms(&v)?,
                _ => 0,
            };
            let body = body.unwrap_or_default();
            let verb = verb.to_ascii_uppercase();
            let job = request(&st, f, verb, url, hdrs, body, timeout_ms);
            let st = st.clone();
            let cancel = method(move |_, _| {
                if let Some(job) = &job {
                    with(&st, |s| {
                        if let Some(t) = job.0.timer.take() {
                            s.cancel_timer(t);
                        }
                        if let Some(id) = job.1 {
                            s.net.waiting.remove(&id);
                        }
                        done(s, &job.0);
                    });
                }
                Ok(nil())
            });
            new_object(lua, "job", &[("cancel", cancel)])?.into_lua_multi(lua)
        })
    };
    let keep_wifi = {
        let st = st.clone();
        method(move |_, args| {
            let a = args.into_vec();
            let v = arg(&a, 0);
            check_arg("keep_wifi", 1, v, "number")?;
            let ms = match v {
                Value::Integer(i) => f64::from(*i),
                Value::Number(n) => widen(*n),
                _ => 0.0,
            };
            with(&st, |s| {
                s.log_line(&format!("net: keep_wifi {} ms", py_g(ms)))
            });
            Ok(nil())
        })
    };
    let websocket = method(|_, _| Err(mlua::Error::runtime("ctx.net:websocket is not emulated")));
    let net = new_object(
        lua,
        "net",
        &[
            ("request", request),
            ("keep_wifi", keep_wifi),
            ("websocket", websocket),
        ],
    )?;
    ctx.raw_set("net", net)
}

/// What [`request`] does after its first step.
enum Next {
    Done(Option<(Rc<Job>, Option<u64>)>),
    Fetch(Rc<Job>, OutRequest, Fetcher),
}

/// 0.1 `Net.request` plus the timer: None when the request never gets an answer (an unsupported method), else
/// the job and, for a request left in the outbox, its id.
fn request(
    st: &Shared,
    f: Function,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    timeout_ms: i64,
) -> Option<(Rc<Job>, Option<u64>)> {
    match first_step(st, f, method, url, headers, body, timeout_ms) {
        Next::Done(r) => r,
        Next::Fetch(job, out, mut fetcher) => {
            // the fetcher is out of the state while it runs, so no borrow is held
            let fetched = fetcher(&out);
            with(st, |s| {
                if s.net.fetcher.is_none() {
                    s.net.fetcher = Some(fetcher);
                }
                for note in fetched.notes {
                    match note {
                        NetNote::Log(l) => s.log_line(&l),
                        NetNote::Warn(w) => s.warn(&w),
                    }
                }
                let delay = delay_of(&fetched.result);
                schedule(s, job.clone(), fetched.result, delay);
            });
            Some((job, None))
        }
    }
}

fn first_step(
    st: &Shared,
    f: Function,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    timeout_ms: i64,
) -> Next {
    with(st, |s| {
        let red = s.net.redactor.clone();
        let shown = red.redact_str(&url);
        if method != "GET" && method != "POST" {
            s.warn(&format!(
                "net: {method} {shown} never gets an answer on the reader (only GET and POST work); \
                 send it as POST with an override header"
            ));
            return Next::Done(None);
        }
        let job = Rc::new(Job {
            f,
            t0: s.sched.clock.ms,
            active: Cell::new(true),
            timer: Cell::new(None),
        });
        s.net.inflight += 1;
        let blocked = !s.cfg.secret_problems.is_empty() && s.cfg.net_mode == NetMode::Record;
        if blocked {
            schedule(
                s,
                job.clone(),
                NetResult::Error("secrets_invalid".into()),
                DEFAULT_DELAY_MS,
            );
            return Next::Done(Some((job, None)));
        }
        let sha = red.body_sha(&method, &body);
        let mut warnings = Vec::new();
        let found = if s.cfg.net_mode == NetMode::Live || s.cfg.net_mode == NetMode::Record {
            None
        } else {
            s.net
                .fixtures
                .clone()
                .find(&method, &shown, sha.as_deref(), &mut |w| warnings.push(w))
        };
        for w in warnings {
            s.warn(&w);
        }
        let goes_out = match s.cfg.net_mode {
            NetMode::Replay => false,
            NetMode::FixturesThenLive => found.is_none(),
            NetMode::Live | NetMode::Record => true,
        };
        if goes_out {
            s.net.next_id += 1;
            let id = s.net.next_id;
            let out = OutRequest {
                id,
                method,
                url,
                headers,
                body,
                timeout_ms,
            };
            if let Some(fetcher) = s.net.fetcher.take() {
                return Next::Fetch(job, out, fetcher);
            }
            s.net.outbox.push(out);
            s.net.waiting.insert(id, job.clone());
            return Next::Done(Some((job, Some(id))));
        }
        let resp = match found {
            Some(r) => {
                if let NetResult::Response { status, .. } = &r {
                    s.log_line(&format!("net: {method} {shown} -> {status} (fixture)"));
                }
                r
            }
            None => {
                let msg = format!(
                    "net: no fixture for {method} {shown}; run with --net record to save one, or write it by hand in {}",
                    s.cfg.net_fixtures_label
                );
                s.warn(&msg);
                NetResult::Error("no_fixture".into())
            }
        };
        let delay = delay_of(&resp);
        schedule(s, job.clone(), resp, delay);
        Next::Done(Some((job, None)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_format_as_python() {
        for (x, want) in [
            (300000.0, "300000"),
            (1234567.0, "1.23457e+06"),
            (0.5, "0.5"),
            (1e-5, "1e-05"),
            (0.0, "0"),
            (-2.5, "-2.5"),
            (100.0, "100"),
        ] {
            assert_eq!(py_g(x), want);
        }
    }

    #[test]
    fn url_check_as_0_1() {
        assert!(url_ok("http://x.test/a"));
        assert!(url_ok("HTTPS://x"));
        assert!(!url_ok("nonsense"));
        assert!(!url_ok("http:///a"));
        assert!(!url_ok("ftp://x"));
    }
}
