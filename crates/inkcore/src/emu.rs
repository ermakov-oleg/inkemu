//! The emulator core: one app, its Lua state, virtual time and lifecycle, as 0.1 `emu.Emu`.
use std::cell::{Ref, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Table, Value};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::app::{app_id_of, read_manifest, AppFiles};
use crate::clock::{iso, DateTime, Due, Run, Scheduler, TimerId, VirtualClock};
use crate::config::{Config, Persisted};
use crate::ctx::net::{self, NetResult, NetState, OutRequest};
use crate::dialog::Handler;
use crate::draw::image::Gray;
use crate::draw::plates::{self, Overlays, Plate};
use crate::draw::{Canvas, Frame, Target, TextItem};
use crate::fixtures::{FixtureSet, Redactor};
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
    /// `ctx.image`, `ctx.qr` and `ctx.layers` images by `_id`, until released; dropped with the Lua state.
    pub(crate) images: HashMap<i64, Rc<Gray>>,
    pub(crate) next_image: i64,
    pub(crate) frame_commands: usize,
    pub(crate) locked: bool,
    pub(crate) lock_wakes: u32,
    pub(crate) refresh_counts: (u32, u32),
    pub(crate) frontlight: i64,
    pub(crate) full_refresh_deferred: i64,
    /// For `ctx.random`; lives as long as the core, so uuids never repeat across restarts.
    pub(crate) rng: ChaCha20Rng,
    pub(crate) persisted: Persisted,
    pub(crate) persist_changed: bool,
    pub(crate) net: NetState,
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
        self.net.stop();
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
        crate::ctx::save_state(self);
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
    pub fn new(app: AppFiles, mut cfg: Config, persisted: Persisted) -> Core {
        let app_id = app_id_of(
            &read_manifest(&app, cfg.firmware_api, &cfg.app_dir_label).0,
            &cfg.fallback_app_id,
        );
        let clock = VirtualClock::new(cfg.start_local_ms);
        let net = NetState::new(
            FixtureSet::from_files(std::mem::take(&mut cfg.fixtures)),
            Redactor::new(&cfg.secrets),
        );
        let rng = match cfg.random_seed {
            Some(seed) => ChaCha20Rng::seed_from_u64(seed),
            None => {
                let mut seed = [0u8; 32];
                getrandom::fill(&mut seed).expect("no OS entropy for ctx.random");
                ChaCha20Rng::from_seed(seed)
            }
        };
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
            images: HashMap::new(),
            next_image: 0,
            frame_commands: 0,
            locked: false,
            lock_wakes: 0,
            refresh_counts: (0, 0),
            frontlight: 0,
            full_refresh_deferred: 0,
            rng,
            persisted,
            persist_changed: false,
            net,
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
            (s.images, s.next_image) = (HashMap::new(), 0);
            s.net.inflight = 0;
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
        let permitted = |name: &str| {
            manifest
                .get("permissions")
                .and_then(|p| p.as_array())
                .is_some_and(|p| p.iter().any(|v| v == name))
        };
        let (ctx, g) = self
            .install(permitted("reader"), permitted("net"))
            .expect("cannot set up the app's Lua state");
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
    fn install(&self, reader: bool, net: bool) -> mlua::Result<(Table, Table)> {
        let lua = &self.lua;
        lua::install_require(lua, self.app.clone())?;
        let ctx = crate::ctx::new_ctx(lua, &self.st, &self.app, reader, net)?;
        lua.globals().set("ctx", &ctx)?;
        let g = crate::draw::new_g(lua, &self.st, Target::Screen)?;
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

    // ---- network ------------------------------------------------------------------------------

    /// `record`/`live` requests made since the last call, for the host to perform.
    pub fn take_outgoing(&mut self) -> Vec<OutRequest> {
        with(&self.st, |s| s.net.take_outgoing())
    }

    /// The host's answer to an outgoing request; the callback fires at request time + delay, at once if that
    /// has passed.
    pub fn deliver(&mut self, id: u64, result: NetResult) {
        net::deliver(&self.host(), id, result);
    }

    /// Requests the app still waits for.
    pub fn net_inflight(&self) -> u32 {
        self.st.borrow().net.inflight
    }

    /// Replaces the secrets (as `Config.secrets` and `Config.secret_problems`); the next start warns about the
    /// problems, and the redaction applies to every request from now on.
    pub fn set_secrets(&mut self, secrets: Vec<(String, String)>, problems: Vec<String>) {
        with(&self.st, |s| {
            s.net.redactor = Rc::new(Redactor::new(&secrets));
            s.cfg.secrets = secrets;
            s.cfg.secret_problems = problems;
        });
    }

    /// Replaces the `<fixtures>/net/` files `replay` answers from.
    pub fn set_fixtures(&mut self, files: std::collections::BTreeMap<String, Vec<u8>>) {
        with(&self.st, |s| {
            s.net.fixtures = Rc::new(FixtureSet::from_files(files))
        });
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

    /// A warning: in `warnings` and the log as "WARN ...".
    pub fn warn(&mut self, msg: &str) {
        with(&self.st, |s| s.warn(msg));
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
        let first = r.iter().next().unwrap_or(&Value::Nil);
        Ok(
            crate::ctx::convert::lua_to_json(first, "lua_eval", None, &mut |_| {})
                .unwrap_or(serde_json::Value::Null),
        )
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
