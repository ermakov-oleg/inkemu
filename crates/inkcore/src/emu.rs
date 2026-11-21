//! The emulator core: one app, its Lua state, virtual time and lifecycle, as 0.1 `emu.Emu`.
use std::cell::{Ref, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Table, Value};

use crate::app::{app_id_of, read_manifest, AppFiles};
use crate::clock::{iso, DateTime, Due, Run, Scheduler, TimerId, VirtualClock};
use crate::config::{Config, Persisted};
use crate::dialog::Handler;
use crate::draw::plates::{self, Overlays, Plate};
use crate::draw::{Canvas, Frame, TextItem};
use crate::log::{py_repr, Log};
use crate::lua;

pub const ERROR_TITLE: &str = "App error, closed";
const INSTALL_FAILED: &str = "Install failed";
/// What the not-yet-ported lock methods return; the Python binding maps it to NotImplementedError.
pub const NOT_YET: &str = "not implemented yet";

/// A dialog waiting for the user.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub kind: &'static str,
    pub title: String,
    pub text: String,
}

/// 0.1 `Emu.status()`.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub time: String,
    pub net: &'static str,
    pub frame_commands: usize,
    pub paused: bool,
    pub screen: (u32, u32),
    pub app_id: String,
    pub running: bool,
    pub pending: Option<Pending>,
    pub locked: bool,
}

/// What a timer does when it fires.
pub(crate) type TimerFn = Rc<dyn Fn(&Host)>;

/// Core state shared with the Rust functions the app calls through Lua.
pub(crate) struct State {
    pub(crate) cfg: Config,
    pub(crate) log: Log,
    pub(crate) sched: Scheduler<TimerFn>,
    /// The app's timers, cancelled when it stops.
    pub(crate) timers: BTreeSet<TimerId>,
    pub(crate) app_id: String,
    pub(crate) ctx: Option<Table>,
    pub(crate) g: Option<Table>,
    pub(crate) running: bool,
    pub(crate) paused: bool,
    pub(crate) invalid: bool,
    pub(crate) quit: bool,
    pub(crate) leaving: bool,
    pub(crate) depth: u32,
    pub(crate) overlay: Option<(String, String)>,
    pub(crate) pending: Option<Pending>,
    /// What answering or typing into `pending` does; set and cleared with it.
    pub(crate) pending_h: Option<Handler>,
    pub(crate) pending_id: u64,
    pub(crate) tick: Option<TimerId>,
    /// Pointer down position and clock ms, until the matching up.
    pub(crate) down: Option<(i32, i32, i64)>,
    pub(crate) hold_sent: bool,
    /// Text and the clock ms it stays up to.
    pub(crate) toast: Option<(String, i64)>,
    pub(crate) canvas: Canvas,
    pub(crate) frame_commands: usize,
    pub(crate) locked: bool,
    pub(crate) lock_wakes: u32,
    pub(crate) refresh_counts: (u32, u32),
    pub(crate) persisted: Persisted,
    pub(crate) persist_changed: bool,
}

impl State {
    pub(crate) fn log_line(&mut self, msg: &str) {
        let now = self.sched.clock.now();
        self.log.line(now, msg);
    }

    pub(crate) fn warn(&mut self, msg: &str) {
        self.log.warnings.push(msg.to_string());
        self.log_line(&format!("WARN {msg}"));
    }

    pub(crate) fn stop(&mut self) {
        self.running = false;
        self.clear_pending();
        self.invalid = false;
        for t in std::mem::take(&mut self.timers) {
            self.sched.cancel(t);
        }
        self.tick = None;
    }

    pub(crate) fn clear_pending(&mut self) {
        self.pending = None;
        self.pending_h = None;
    }

    /// An app timer: cancelled when the app stops.
    pub(crate) fn add_timer(&mut self, delay_ms: i64, f: TimerFn, period_ms: i64) -> TimerId {
        let id = self.sched.add(delay_ms, f, period_ms);
        self.timers.insert(id);
        id
    }

    pub(crate) fn cancel_timer(&mut self, id: TimerId) {
        self.sched.cancel(id);
        self.timers.remove(&id);
    }

    /// The app's text items, then the plates' (error overlay, dialog, toast).
    pub(crate) fn texts(&self) -> Vec<TextItem> {
        let mut out = self.canvas.texts();
        out.extend(self.plates(true).into_iter().flat_map(|p| p.items));
        out
    }

    pub(crate) fn fail(&mut self, message: &str, title: &str) {
        self.log.errors.push(message.to_string());
        self.log_line(&format!("ERROR {message}"));
        let first = message.split('\n').next().unwrap_or_default();
        self.overlay = Some((title.to_string(), first.to_string()));
        self.stop();
    }

    pub(crate) fn plates(&self, dialog: bool) -> Vec<Plate> {
        let now = self.sched.clock.ms;
        let o = Overlays {
            error: self.overlay.as_ref(),
            pending: self.pending.as_ref(),
            toast: self
                .toast
                .as_ref()
                .filter(|t| now < t.1)
                .map(|t| t.0.as_str()),
        };
        plates::plates(&self.canvas, &o, dialog)
    }
}

pub(crate) type Shared = Rc<RefCell<State>>;

/// Runs `f` on the shared state. The borrow ends when `f` returns: never call into Lua inside `f`.
pub(crate) fn with<R>(st: &Shared, f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut st.borrow_mut())
}

/// The Lua state and the shared core state: all a host function needs to call back into the app.
pub(crate) struct Host<'a> {
    pub lua: &'a Lua,
    pub st: &'a Shared,
}

impl Host<'_> {
    /// Calls `f` under the instruction budget; a Lua error lands in `errors`, shows the error overlay and stops
    /// the app. None when the app is not running or the call failed.
    pub fn protected(
        &self,
        f: &Function,
        args: impl IntoLuaMulti,
        where_: &str,
    ) -> Option<MultiValue> {
        let running = with(self.st, |s| {
            s.depth += u32::from(s.running);
            s.running
        });
        if !running {
            return None;
        }
        let r = lua::call_guarded(self.lua, f, args);
        let quit_now = with(self.st, |s| {
            s.depth -= 1;
            match &r {
                // an outer callback unwinding after a nested failure adds nothing
                Err(e) if s.running || s.overlay.is_none() => {
                    s.fail(&format!("Lua error in {where_}: {e}"), ERROR_TITLE);
                    false
                }
                Err(_) => false,
                Ok(_) => s.quit && s.depth == 0 && !s.leaving,
            }
        });
        if quit_now {
            self.leave();
        }
        r.ok()
    }

    /// Calls the app's global function `name` if it is defined; its first result.
    pub fn call(&self, name: &str, args: impl IntoLuaMulti) -> Option<Value> {
        if !with(self.st, |s| s.running) {
            return None;
        }
        // raw: an app metatable on _G must not run here
        let Ok(Value::Function(f)) = self.lua.globals().raw_get::<Value>(name) else {
            return None;
        };
        self.protected(&f, args, name)?.into_iter().next()
    }

    /// Like [`Host::protected`] for a value the app gave as a callback; calling a non-function fails the app.
    pub fn protected_value(
        &self,
        f: &Value,
        args: impl IntoLuaMulti,
        where_: &str,
    ) -> Option<MultiValue> {
        match f {
            Value::Function(f) => self.protected(f, args, where_),
            other => {
                let ty = match other {
                    Value::Integer(_) | Value::Number(_) => "number",
                    v => v.type_name(),
                };
                with(self.st, |s| {
                    if s.running {
                        let msg = format!("Lua error in {where_}: attempt to call a {ty} value");
                        s.fail(&msg, ERROR_TITLE);
                    }
                });
                None
            }
        }
    }

    pub fn ctx(&self) -> Option<Table> {
        with(self.st, |s| s.ctx.clone())
    }

    /// Runs virtual time forward, firing due timers and redrawing as needed. Safe to nest: each run keeps its
    /// own per-instant counter.
    pub fn wait(&self, ms: i64) -> Result<(), String> {
        if ms < 0 {
            return Err("wait() needs a non-negative duration".into());
        }
        let target = with(self.st, |s| s.sched.clock.ms.saturating_add(ms));
        let mut run = Run::default();
        while let Some(due) = with(self.st, |s| s.sched.pop_due(target, &mut run)) {
            match due {
                Due::Overflow(msg) => with(self.st, |s| s.fail(&msg, ERROR_TITLE)),
                Due::Fire {
                    id,
                    payload,
                    periodic,
                } => {
                    if !periodic {
                        with(self.st, |s| s.timers.remove(&id));
                    }
                    payload(self);
                    self.settle();
                    with(self.st, |s| s.sched.rearm(id));
                }
            }
        }
        with(self.st, |s| s.sched.advance_to(target));
        self.settle();
        Ok(())
    }

    pub fn leave(&self) {
        let go = with(self.st, |s| {
            let go = s.running && !s.leaving;
            s.leaving |= go;
            go
        });
        if !go {
            return;
        }
        self.call("on_leave", self.ctx());
        if with(self.st, |s| s.running) {
            self.save_state();
        }
        with(self.st, |s| {
            s.leaving = false;
            s.quit = false;
            s.stop();
            s.log_line("app left");
        });
    }

    /// Serialises `ctx.state` into the persisted data (0.1 `save_state`); the lock screen never overwrites it.
    pub fn save_state(&self) {
        let Some(ctx) = self.ctx() else { return };
        if with(self.st, |s| s.locked) {
            return;
        }
        let state = ctx.raw_get::<Value>("state").unwrap_or(Value::Nil);
        let data = match state {
            Value::Nil => serde_json::json!({}),
            Value::Table(_) => to_json(&state, 0),
            other => {
                let msg = format!(
                    "ctx.state is a {}, not a table: not saved",
                    other.type_name()
                );
                with(self.st, |s| s.warn(&msg));
                return;
            }
        };
        let bytes = serde_json::to_vec(&data).unwrap_or_default();
        with(self.st, |s| {
            if s.persisted.state_json.as_deref() != Some(bytes.as_slice()) {
                s.persisted.state_json = Some(bytes);
                s.persist_changed = true;
            }
        });
    }

    pub fn settle(&self) {
        let draw = with(self.st, |s| std::mem::take(&mut s.invalid) && s.running);
        if draw {
            self.draw();
        }
    }

    fn draw(&self) {
        let (ctx, g) = with(self.st, |s| {
            s.canvas.begin_frame();
            (s.ctx.clone(), s.g.clone())
        });
        self.call("on_draw", (ctx, g));
        with(self.st, |s| {
            let n = s.canvas.end_frame();
            for w in s.canvas.take_warnings() {
                s.warn(&w);
            }
            match s.canvas.take_bad() {
                Some(bad) => {
                    s.fail(&format!("g: bad values in on_draw: {bad}"), ERROR_TITLE);
                    s.canvas.begin_frame();
                }
                None => s.frame_commands = n,
            }
        });
    }
}

/// One emulated InkApp.
pub struct Core {
    st: Shared,
    lua: Lua,
    app: Rc<AppFiles>,
}

impl Core {
    /// Boots the app: manifest, fresh Lua state, entry chunk, `on_load`, first frame.
    pub fn new(app: AppFiles, cfg: Config, persisted: Persisted) -> Core {
        let app_id = app_id_of(
            &read_manifest(&app, cfg.firmware_api, &cfg.app_dir_label).0,
            &cfg.fallback_app_id,
        );
        let clock = VirtualClock::new(cfg.start_local_ms);
        let st = Rc::new(RefCell::new(State {
            cfg,
            log: Log::default(),
            sched: Scheduler::new(clock),
            timers: BTreeSet::new(),
            app_id,
            ctx: None,
            g: None,
            running: false,
            paused: false,
            invalid: false,
            quit: false,
            leaving: false,
            depth: 0,
            overlay: None,
            pending: None,
            pending_h: None,
            pending_id: 0,
            tick: None,
            down: None,
            hold_sent: false,
            toast: None,
            canvas: Canvas::new(480, 800),
            frame_commands: 0,
            locked: false,
            lock_wakes: 0,
            refresh_counts: (0, 0),
            persisted,
            persist_changed: false,
        }));
        let mut core = Core {
            st,
            lua: lua::new_lua(),
            app: Rc::new(app),
        };
        core.boot();
        core
    }

    fn host(&self) -> Host<'_> {
        Host {
            lua: &self.lua,
            st: &self.st,
        }
    }

    fn boot(&mut self) {
        let (fw, label, budget) = with(&self.st, |s| {
            s.stop();
            (s.overlay, s.ctx, s.g, s.quit, s.toast) = (None, None, None, false, None);
            (
                s.cfg.firmware_api,
                s.cfg.app_dir_label.clone(),
                s.cfg.instruction_budget,
            )
        });
        let (manifest, problem) = read_manifest(&self.app, fw, &label);
        let landscape = manifest
            .get("display")
            .and_then(|d| d.get("orientation"))
            .is_some_and(|o| o == "landscape");
        self.lua = lua::new_lua();
        lua::set_budget(&self.lua, budget);
        with(&self.st, |s| {
            s.canvas = if landscape {
                Canvas::new(800, 480)
            } else {
                Canvas::new(480, 800)
            };
            s.frame_commands = 0;
        });
        if let Some(p) = problem {
            with(&self.st, |s| s.fail(&p, INSTALL_FAILED));
            return;
        }
        let entry = manifest
            .get("entry")
            .cloned()
            .unwrap_or_else(|| "index.lua".into());
        let app = self.app.clone();
        let Some((found, src)) = entry.as_str().and_then(|e| app.find(e)) else {
            let msg = format!(
                "manifest.json: entry {} not found in the app folder",
                py_repr(&entry)
            );
            with(&self.st, |s| s.fail(&msg, INSTALL_FAILED));
            return;
        };
        let (ctx, g) = self.install().expect("cannot set up the app's Lua state");
        with(&self.st, |s| (s.ctx, s.g) = (Some(ctx.clone()), Some(g)));
        let f = match lua::load_chunk(&self.lua, src, &format!("@{found}")) {
            Ok(f) => f,
            Err(e) => {
                with(&self.st, |s| s.fail(&format!("{found}: {e}"), ERROR_TITLE));
                return;
            }
        };
        with(&self.st, |s| {
            s.running = true;
            s.invalid = true;
            let msg = format!("app {} started", s.app_id);
            s.log_line(&msg);
        });
        let h = self.host();
        h.protected(&f, (), &found);
        h.call("on_load", ctx);
        h.settle();
    }

    /// `require`, the global `ctx` and the `g` passed to `on_draw`.
    fn install(&self) -> mlua::Result<(Table, Table)> {
        let lua = &self.lua;
        lua::install_require(lua, self.app.clone())?;
        let ctx = crate::ctx::new_ctx(lua, &self.st)?;
        lua.globals().set("ctx", &ctx)?;
        let saved = with(&self.st, |s| s.persisted.state_json.clone());
        ctx.set("state", load_state(lua, saved.as_deref())?)?;
        let g = crate::draw::new_g(lua, &self.st)?;
        Ok((ctx, g))
    }

    // ---- input --------------------------------------------------------------------------------

    /// Low-level pointer event; `up` classifies the gesture and advances 350 ms.
    pub fn pointer(&mut self, phase: &str, x: i32, y: i32) -> Result<(), String> {
        self.host().pointer(phase, x, y)
    }

    pub fn tap(&mut self, x: i32, y: i32) -> Result<(), String> {
        self.host().tap(x, y)
    }

    pub fn double_tap(&mut self, x: i32, y: i32) -> Result<(), String> {
        self.host().double_tap(x, y)
    }

    pub fn hold(&mut self, x: i32, y: i32, ms: i64) -> Result<(), String> {
        self.host().hold(x, y, ms)
    }

    pub fn drag(&mut self, points: &[(i32, i32)], duration_ms: i64) -> Result<(), String> {
        self.host().drag(points, duration_ms)
    }

    pub fn swipe(&mut self, dir: &str, at: (i32, i32), distance: i32) -> Result<(), String> {
        self.host().swipe(dir, at, distance)
    }

    /// Taps the last drawn text equal to `text`, else the last containing it.
    pub fn tap_text(&mut self, text: &str) -> Result<(), String> {
        self.host().tap_text(text)
    }

    pub fn key(&mut self, name: &str) -> Result<(), String> {
        self.host().key(name)
    }

    /// Answers a pending `input:text` with OK, or types into a pending `input:edit` and presses enter.
    pub fn type_text(&mut self, text: &str) -> Result<(), String> {
        self.host().type_text(text)
    }

    /// Answers the pending dialog: confirm yes/no, leave yes/no, text OK/cancel.
    pub fn answer(&mut self, ok: bool) -> Result<(), String> {
        self.host().answer(ok)
    }

    // ---- time and lifecycle --------------------------------------------------------------------

    /// Runs virtual time forward, firing due timers and redrawing as needed.
    pub fn wait(&mut self, ms: i64) -> Result<(), String> {
        self.host().wait(ms)
    }

    pub fn leave(&mut self) {
        self.host().leave();
    }

    pub fn restart(&mut self) {
        self.leave();
        self.boot();
    }

    pub fn reload(&mut self) {
        if with(&self.st, |s| s.running) {
            self.host().save_state();
        }
        self.boot();
    }

    /// Replaces the app folder's contents; `reload()` then runs the new code.
    pub fn set_app(&mut self, app: AppFiles) {
        self.app = Rc::new(app);
    }

    pub fn lock(&mut self) -> Result<(), String> {
        Err(NOT_YET.into())
    }

    pub fn unlock(&mut self) -> Result<(), String> {
        Err(NOT_YET.into())
    }

    pub fn resume(&mut self) {
        with(&self.st, |s| s.paused = false);
    }

    /// The window and the daemon freeze the clock with this; only `status()` reports it.
    pub fn set_paused(&mut self, paused: bool) {
        with(&self.st, |s| s.paused = paused);
    }

    // ---- inspection ----------------------------------------------------------------------------

    pub fn status(&self) -> Status {
        let s = self.st.borrow();
        Status {
            time: iso(s.sched.clock.now()),
            net: s.cfg.net_mode.as_str(),
            frame_commands: s.frame_commands,
            paused: s.paused,
            screen: (s.canvas.w, s.canvas.h),
            app_id: s.app_id.clone(),
            running: s.running,
            pending: s.pending.clone(),
            locked: s.locked,
        }
    }

    /// The app's text items, then the plates' (error overlay, dialog, toast).
    pub fn texts(&self) -> Vec<TextItem> {
        self.st.borrow().texts()
    }

    /// The screen with the error, toast and (if `dialog`) dialog plates.
    pub fn frame(&self, dialog: bool) -> Frame {
        let s = self.st.borrow();
        let mut f = s.canvas.render();
        plates::draw(&s.canvas, &mut f, &s.plates(dialog));
        f
    }

    /// The app's own frame, without plates.
    pub fn raw_frame(&self) -> Frame {
        self.st.borrow().canvas.render()
    }

    pub fn log(&self) -> Ref<'_, [String]> {
        Ref::map(self.st.borrow(), |s| s.log.log.as_slice())
    }

    pub fn errors(&self) -> Ref<'_, [String]> {
        Ref::map(self.st.borrow(), |s| s.log.errors.as_slice())
    }

    pub fn warnings(&self) -> Ref<'_, [String]> {
        Ref::map(self.st.borrow(), |s| s.log.warnings.as_slice())
    }

    pub fn toasts(&self) -> Ref<'_, [String]> {
        Ref::map(self.st.borrow(), |s| s.log.toasts.as_slice())
    }

    pub fn log_line(&mut self, msg: &str) {
        with(&self.st, |s| s.log_line(msg));
    }

    /// The app data to write to disk, when state, files or lock data changed since the last take.
    pub fn take_persist(&mut self) -> Option<Persisted> {
        with(&self.st, |s| {
            std::mem::take(&mut s.persist_changed).then(|| s.persisted.clone())
        })
    }

    /// Runs Lua code in the app's state under the budget (test helper).
    pub fn lua_exec(&mut self, code: &str) -> Result<(), String> {
        let f = lua::load_chunk(&self.lua, code.as_bytes(), "=lua_exec")?;
        lua::call_guarded(&self.lua, &f, ()).map(|_| ())
    }

    /// Runs Lua code in the app's state under the budget and returns its first result as JSON (test helper).
    pub fn lua_eval(&mut self, code: &str) -> Result<serde_json::Value, String> {
        let f = lua::load_chunk(&self.lua, code.as_bytes(), "=lua_eval")?;
        let r = lua::call_guarded(&self.lua, &f, ())?;
        Ok(to_json(r.iter().next().unwrap_or(&Value::Nil), 0))
    }

    /// Milliseconds of virtual time since start.
    pub fn clock_ms(&self) -> i64 {
        self.st.borrow().sched.clock.ms
    }

    /// Local wall-clock time now.
    pub fn now_local(&self) -> DateTime {
        self.st.borrow().sched.clock.now()
    }

    /// (requested, full) screen refreshes.
    pub fn refresh_counts(&self) -> (u32, u32) {
        self.st.borrow().refresh_counts
    }

    pub fn set_locale(&mut self, locale: &str) {
        with(&self.st, |s| s.cfg.locale = locale.to_string());
    }

    pub fn lock_wakes(&self) -> u32 {
        self.st.borrow().lock_wakes
    }

    pub fn frame_commands(&self) -> usize {
        self.st.borrow().frame_commands
    }

    /// Changes whenever a new dialog becomes pending.
    pub fn pending_id(&self) -> u64 {
        self.st.borrow().pending_id
    }
}

/// `ctx.state` from the saved JSON; an unreadable or non-table file starts empty (0.1 also logs a warning).
fn load_state(lua: &Lua, saved: Option<&[u8]>) -> mlua::Result<Value> {
    match saved.and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok()) {
        Some(v @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))) => {
            from_json(lua, &v)
        }
        _ => Ok(Value::Table(lua.create_table()?)),
    }
}

fn from_json(lua: &Lua, v: &serde_json::Value) -> mlua::Result<Value> {
    use serde_json::Value as J;
    Ok(match v {
        J::Null => Value::Nil,
        J::Bool(b) => Value::Boolean(*b),
        J::Number(n) => match n.as_i64().and_then(|i| i32::try_from(i).ok()) {
            Some(i) => Value::Integer(i),
            None => Value::Number(n.as_f64().unwrap_or(0.0) as _),
        },
        J::String(s) => Value::String(lua.create_string(s)?),
        J::Array(a) => {
            let t = lua.create_table()?;
            for (i, x) in a.iter().enumerate() {
                t.raw_set(i + 1, from_json(lua, x)?)?;
            }
            Value::Table(t)
        }
        J::Object(o) => {
            let t = lua.create_table()?;
            for (k, x) in o {
                t.raw_set(k.as_str(), from_json(lua, x)?)?;
            }
            Value::Table(t)
        }
    })
}

/// Plain Lua values as JSON for the test helpers; tables become arrays when they have a sequence part.
fn to_json(v: &Value, depth: u32) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        Value::Nil => J::Null,
        Value::Boolean(b) => J::Bool(*b),
        Value::Integer(i) => J::from(*i),
        Value::Number(n) => serde_json::Number::from_f64(f64::from(*n)).map_or(J::Null, J::Number),
        Value::String(s) => J::String(s.to_string_lossy()),
        Value::Table(t) if depth < 32 => {
            let n = t.raw_len();
            if n > 0 {
                (1..=n)
                    .map(|i| to_json(&t.raw_get::<Value>(i).unwrap_or(Value::Nil), depth + 1))
                    .collect()
            } else {
                t.pairs::<Value, Value>()
                    .flatten()
                    .map(|(k, v)| {
                        let k = match k {
                            Value::String(s) => s.to_string_lossy(),
                            k => k.to_string().unwrap_or_default(),
                        };
                        (k, to_json(&v, depth + 1))
                    })
                    .collect::<serde_json::Map<_, _>>()
                    .into()
            }
        }
        other => J::String(other.type_name().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0.1's host could call `wait()` from its `on_pending` hook while the app callback that opened the dialog was
    /// still running; the Rust API cannot, so a test-only global stands in for that host.
    #[test]
    fn timer_error_in_a_wait_nested_inside_a_callback() {
        let manifest = br#"{"app_id": "testapp", "entry": "index.lua", "api": "2.2"}"#;
        let index = b"msg = 'none'\n\
            function on_load(c)\n\
              c:after(100, function() error('late') end)\n\
              c:after(300, function() msg = 'second' end)\n\
            end\n\
            function on_input(c, ev)\n\
              if ev.phase == 'up' then c.dialog:confirm({title = 'Sure?'}, function() end) host_wait(500) after = 'resumed' end\n\
              return true\n\
            end\n\
            function on_draw(c, g) g:text(0, 0, 'msg ' .. msg) end\n";
        let files = AppFiles(
            [
                ("manifest.json".to_string(), manifest.to_vec()),
                ("index.lua".to_string(), index.to_vec()),
            ]
            .into(),
        );
        let mut core = Core::new(files, Config::default(), Persisted::default());
        let st = core.st.clone();
        let host_wait = core
            .lua
            .create_function(move |lua, ms: i64| {
                Host { lua, st: &st }.wait(ms).map_err(mlua::Error::runtime)
            })
            .unwrap();
        core.lua.globals().set("host_wait", host_wait).unwrap();
        core.key("ok").unwrap();
        let errors = core.errors().to_vec();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].starts_with("Lua error in ctx:after callback: index.lua:3: late"),
            "{errors:?}"
        );
        let st = core.status();
        assert!(!st.running && st.pending.is_none());
        assert_eq!(core.lua_eval("return after").unwrap(), "resumed");
        assert_eq!(core.lua_eval("return msg").unwrap(), "none");
        assert_eq!(core.clock_ms(), 500 + 350);
    }
}
