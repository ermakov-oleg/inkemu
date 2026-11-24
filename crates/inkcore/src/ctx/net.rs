//! `ctx.net` (0.1 `net.install`): `replay` is answered here from fixtures, and so is `fixtures+live` when a
//! fixture matches. `record` and `live` requests, and `fixtures+live` ones without a fixture, go to the host's
//! [`Fetcher`] at request time, as 0.1's synchronous fetch; a host without one (the web page) takes them from an
//! outbox and hands the answer back through [`Core::deliver`].
//!
//! [`Core::deliver`]: crate::Core::deliver
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Table, Value};

use super::files::{store_capped, Dir, Mode};
use super::object::{method, new_object};
use super::{arg, check_arg, int_value, raw, soft_fail, whole};
use crate::clock::TimerId;
use crate::config::{NetMode, APP_FILE_SAVE_MAX, STORAGE_WRITE_MAX};
use crate::emu::{with, Host, Shared, State};
use crate::fixtures::{FixtureSet, Redactor};

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

/// The 13 methods of §14, compared byte for byte.
const METHODS: [&str; 13] = [
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
    "PROPFIND",
    "PROPPATCH",
    "MKCOL",
    "COPY",
    "MOVE",
    "REPORT",
];
const URL_MAX: usize = 1024;
const BODY_MAX: usize = 65_536;
const HEADERS_MAX: usize = 16;
const HEADER_NAME_MAX: usize = 64;
const HEADERS_TOTAL_MAX: usize = 65_536;
/// At most this much of an answer's body reaches `res.body`.
const KEPT_BODY_MAX: usize = 131_072;

/// Where `save` puts a 2xx body.
struct Save {
    dir: Dir,
    name: Vec<u8>,
}

/// A request the app is waiting for.
struct Job {
    f: Function,
    /// Clock ms when the app asked.
    t0: i64,
    active: Cell<bool>,
    timer: Cell<Option<TimerId>>,
    head: bool,
    save: Option<Save>,
}

#[derive(Default)]
pub(crate) struct NetState {
    outbox: Vec<OutRequest>,
    /// Outgoing requests by id until the host answers them.
    waiting: HashMap<u64, Rc<Job>>,
    next_id: u64,
    /// Requests the app still waits for (0.1 `net_inflight`): the lock screen waits for them, and a new
    /// request is `busy` while one is unfinished.
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

    /// The app stopped: answers still on their way are dropped (0.1 cancelled their timers), and requests an async
    /// host (the page) has not taken yet are never sent.
    pub(crate) fn stop(&mut self) {
        self.waiting.clear();
        self.outbox.clear();
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

/// The result table's fields.
#[derive(Default)]
struct Answer {
    ok: bool,
    status: Option<i64>,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    bytes: Option<usize>,
    saved: Option<bool>,
    err: Option<String>,
}

/// What the app gets for `resp`; a 2xx body with `save` is stored here, before the callback.
fn answer(s: &mut State, job: &Job, resp: &NetResult) -> Answer {
    let (status, headers, body) = match resp {
        NetResult::Response {
            status,
            headers,
            body,
            ..
        } => (*status, headers, body.as_slice()),
        NetResult::Error(err) => {
            return Answer {
                saved: job.save.as_ref().map(|_| false),
                err: Some(err.clone()),
                ..Answer::default()
            }
        }
    };
    let body = if job.head { &[][..] } else { body };
    let mut a = Answer {
        ok: true,
        status: Some(status),
        headers: headers.clone(),
        bytes: Some(body.len()),
        ..Answer::default()
    };
    if let Some(save) = &job.save {
        if (200..300).contains(&status) {
            let max = match save.dir {
                Dir::Storage => STORAGE_WRITE_MAX,
                Dir::App => APP_FILE_SAVE_MAX,
            };
            let stored = store_capped(s, save.dir, &save.name, body, Mode::Write, max);
            a.saved = Some(stored.is_ok());
            a.err = stored.err().map(String::from);
            a.body = Some(Vec::new());
            return a;
        }
        a.saved = Some(false);
    }
    a.body = Some(body[..body.len().min(KEPT_BODY_MAX)].to_vec());
    if body.len() > KEPT_BODY_MAX {
        a.err = Some("limit_exceeded".into());
    }
    a
}

/// Frees the request's slot, then calls the app's callback with the answer.
fn fire(h: &Host, job: &Job, resp: &NetResult) {
    job.timer.set(None);
    let a = with(h.st, |s| {
        done(s, job);
        answer(s, job, resp)
    });
    if let Ok(t) = response_table(h.lua, &a) {
        h.protected(&job.f, (h.ctx(), t), "ctx.net:request callback");
    }
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

fn response_table(lua: &Lua, a: &Answer) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("ok", a.ok)?;
    if let Some(status) = a.status {
        t.raw_set("status", int_value(status))?;
    }
    let headers = lua.create_table()?;
    for (k, v) in &a.headers {
        headers.raw_set(k.as_str(), v.as_str())?;
    }
    t.raw_set("headers", headers)?;
    if let Some(body) = &a.body {
        t.raw_set("body", lua.create_string(body)?)?;
    }
    if let Some(bytes) = a.bytes {
        t.raw_set("bytes", int_value(bytes as i64))?;
    }
    if let Some(saved) = a.saved {
        t.raw_set("saved", saved)?;
    }
    if let Some(err) = &a.err {
        t.raw_set("err", err.as_str())?;
    }
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

/// A checked option, or the soft failure the call returns.
type Checked<T> = mlua::Result<Result<T, &'static str>>;

/// A request as the app asked for it, checked.
struct Parsed {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    timeout_ms: i64,
    save: Option<Save>,
}

/// The `headers` option: strings (or numbers, as Lua turns them into strings) within the §14 limits.
fn header_pairs(lua: &Lua, v: &Value) -> Checked<Vec<(Vec<u8>, Vec<u8>)>> {
    let t = match v {
        Value::Nil => return Ok(Ok(Vec::new())),
        Value::Table(t) => t,
        _ => return Ok(Err("invalid_argument")),
    };
    let mut out = Vec::new();
    for pair in t.pairs::<Value, Value>() {
        let (k, v) = pair?;
        let Value::String(k) = k else {
            return Ok(Err("invalid_argument"));
        };
        let v = match v {
            Value::String(s) => s.as_bytes().to_vec(),
            n @ (Value::Integer(_) | Value::Number(_)) => match lua.coerce_string(n)? {
                Some(s) => s.as_bytes().to_vec(),
                None => return Ok(Err("invalid_argument")),
            },
            _ => return Ok(Err("invalid_argument")),
        };
        out.push((k.as_bytes().to_vec(), v));
    }
    let bad = |b: &[u8]| b.iter().any(|c| matches!(c, b'\r' | b'\n' | b'\0'));
    let total: usize = out.iter().map(|(k, v)| k.len() + v.len()).sum();
    if out.len() > HEADERS_MAX
        || total > HEADERS_TOTAL_MAX
        || out
            .iter()
            .any(|(k, v)| k.len() > HEADER_NAME_MAX || bad(k) || bad(v))
    {
        return Ok(Err("invalid_argument"));
    }
    Ok(Ok(out))
}

/// The `save` option: a table with `to` = "files" or "storage" and a name valid in that directory.
fn save_target(v: &Value) -> Checked<Option<Save>> {
    let t = match v {
        Value::Nil => return Ok(Ok(None)),
        Value::Table(t) => t,
        _ => return Ok(Err("invalid_argument")),
    };
    let dir = match t.get::<Value>("to")? {
        Value::String(s) if s.as_bytes().as_ref() == b"files" => Dir::App,
        Value::String(s) if s.as_bytes().as_ref() == b"storage" => Dir::Storage,
        _ => return Ok(Err("invalid_argument")),
    };
    match raw(&t.get::<Value>("name")?).filter(|n| dir.name_ok(n)) {
        Some(name) => Ok(Ok(Some(Save { dir, name }))),
        None => Ok(Err("invalid_argument")),
    }
}

/// The options table of `request`; a field of the wrong type is `invalid_argument`, like a bad value.
fn parse(lua: &Lua, st: &Shared, o: &Table) -> Checked<Parsed> {
    let url = match o.get::<Value>("url")? {
        Value::String(s) => String::from_utf8(s.as_bytes().to_vec()).ok(),
        _ => None,
    };
    let Some(url) = url.filter(|u| url_ok(u)) else {
        return Ok(Err("invalid_argument"));
    };
    let method = match o.get::<Value>("method")? {
        Value::Nil => "GET",
        Value::String(s) => match METHODS
            .iter()
            .find(|m| m.as_bytes() == s.as_bytes().as_ref())
        {
            Some(m) => m,
            None => return Ok(Err("invalid_argument")),
        },
        _ => return Ok(Err("invalid_argument")),
    };
    let body = match o.get::<Value>("body")? {
        Value::Nil => None,
        Value::String(s) => Some(s.as_bytes().to_vec()),
        _ => return Ok(Err("invalid_argument")),
    };
    if body.is_some() && (method == "GET" || method == "HEAD") {
        return Ok(Err("invalid_argument"));
    }
    let headers = match header_pairs(lua, &o.get::<Value>("headers")?)? {
        Ok(h) => h,
        Err(code) => return Ok(Err(code)),
    };
    let timeout_ms = match o.get::<Value>("timeout_ms")? {
        // 0: the host's default
        Value::Nil => 0,
        v => match whole(&v).filter(|ms| (1000..=60_000).contains(ms)) {
            Some(ms) => ms,
            None => return Ok(Err("invalid_argument")),
        },
    };
    let save = match save_target(&o.get::<Value>("save")?)? {
        Ok(s) => s,
        Err(code) => return Ok(Err(code)),
    };
    // §14 names no code for these sizes; §18's general rule gives limit_exceeded
    let body = body.unwrap_or_default();
    if url.len() > URL_MAX || body.len() > BODY_MAX {
        return Ok(Err("limit_exceeded"));
    }
    let mut hdrs = Vec::new();
    for (k, v) in headers {
        match (String::from_utf8(k), String::from_utf8(v)) {
            (Ok(k), Ok(v)) => hdrs.push((k, v)),
            _ => with(st, |s| {
                s.warn("net: a header that is not valid UTF-8 was dropped")
            }),
        }
    }
    Ok(Ok(Parsed {
        method: method.to_string(),
        url,
        headers: hdrs,
        body,
        timeout_ms,
        save,
    }))
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
            let p = match parse(lua, &st, &o)? {
                Ok(p) => p,
                Err(code) => return soft_fail(lua, code),
            };
            if with(&st, |s| s.net.inflight > 0) {
                return soft_fail(lua, "busy");
            }
            let job = request(&st, f, p);
            let st = st.clone();
            let cancel = method(move |_, _| {
                with(&st, |s| {
                    if let Some(t) = job.0.timer.take() {
                        s.cancel_timer(t);
                    }
                    if let Some(id) = job.1 {
                        s.net.waiting.remove(&id);
                    }
                    done(s, &job.0);
                });
                Ok(MultiValue::new())
            });
            new_object(lua, "job", &[("cancel", cancel)])?.into_lua_multi(lua)
        })
    };
    let keep_wifi = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            let v = arg(&a, 0);
            check_arg("keep_wifi", 1, v, "number")?;
            let Some(ms) = whole(v).filter(|ms| (-1..=i64::from(i32::MAX)).contains(ms)) else {
                return soft_fail(lua, "invalid_argument");
            };
            with(&st, |s| s.log_line(&format!("net: keep_wifi {ms} ms")));
            Ok(MultiValue::new())
        })
    };
    let websocket = {
        let st = st.clone();
        method(move |lua, _| {
            if with(&st, |s| s.lock.ctx.is_some()) {
                return soft_fail(lua, "inactive");
            }
            Err(mlua::Error::runtime("ctx.net:websocket is not emulated"))
        })
    };
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
    Done(Rc<Job>, Option<u64>),
    Fetch(Rc<Job>, OutRequest, Fetcher),
}

/// 0.1 `Net.request` plus the timer: the job and, for a request left in the outbox, its id.
fn request(st: &Shared, f: Function, p: Parsed) -> (Rc<Job>, Option<u64>) {
    match first_step(st, f, p) {
        Next::Done(job, id) => (job, id),
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
            (job, None)
        }
    }
}

fn first_step(st: &Shared, f: Function, p: Parsed) -> Next {
    let Parsed {
        method,
        url,
        headers,
        body,
        timeout_ms,
        save,
    } = p;
    with(st, |s| {
        let red = s.net.redactor.clone();
        let shown = red.redact_str(&url);
        let job = Rc::new(Job {
            f,
            t0: s.sched.clock.ms,
            active: Cell::new(true),
            timer: Cell::new(None),
            head: method == "HEAD",
            save,
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
            return Next::Done(job, None);
        }
        let sha = red.body_sha(&body);
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
            return Next::Done(job, Some(id));
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
        Next::Done(job, None)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_check_as_0_1() {
        assert!(url_ok("http://x.test/a"));
        assert!(url_ok("HTTPS://x"));
        assert!(!url_ok("nonsense"));
        assert!(!url_ok("http:///a"));
        assert!(!url_ok("ftp://x"));
    }
}
