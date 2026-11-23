//! The emulator core: one app, its Lua state, virtual time and lifecycle, as 0.1 `emu.Emu`.
use std::cell::{Ref, RefCell};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::rc::Rc;

use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Table, Value};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::app::{api_of, app_id_of, read_manifest, AppFiles};
use crate::clock::{iso, DateTime, Due, Run, Scheduler, TimerId, VirtualClock};
use crate::config::{Config, Persisted};
use crate::ctx::net::{self, Fetcher, NetResult, NetState, OutRequest};
use crate::dialog::Handler;
use crate::draw::image::Gray;
use crate::draw::plates::{self, Overlays, Plate};
use crate::draw::text::Fonts;
use crate::draw::{Canvas, Frame, Target, TextItem};
use crate::fixtures::{FixtureSet, Redactor};
use crate::lockscreen::{self, Lock};
use crate::log::{py_repr, Log};
use crate::lua;

pub const ERROR_TITLE: &str = "App error, closed";
const INSTALL_FAILED: &str = "Install failed";
/// Gancao firmware's default: after 29 partial refreshes the 30th frame is a full one.
const AUTO_FULL_EVERY: u32 = 30;
/// How many refresh kinds `Status.refreshes` keeps; status is read on every daemon response and page poll.
const KEPT_REFRESHES: usize = 64;

/// The waveform a frame was presented with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refresh {
    Partial,
    Full,
}

impl Refresh {
    pub fn as_str(self) -> &'static str {
        match self {
            Refresh::Partial => "partial",
            Refresh::Full => "full",
        }
    }
}

/// What `ctx.perf:info()` reports about frames; the timings and the image cache are not emulated.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Perf {
    pub cmd_count: usize,
    pub image_count: usize,
    pub draw_frame: u64,
}

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
    /// The kind of the last presented app frame.
    pub refresh: Option<&'static str>,
    /// The kinds of the last 64 app frames presented by this core, oldest first.
    pub refreshes: Vec<&'static str>,
}

/// What the plates show: error overlay, dialog, visible toast.
pub(crate) type PlatesSig = (Option<(String, String)>, Option<Pending>, Option<String>);

/// What a timer does when it fires.
pub(crate) type TimerFn = Rc<dyn Fn(&Host)>;

/// Core state shared with the Rust functions the app calls through Lua.
pub(crate) struct State {
    /// The running Lua state; replaced at every boot, also by a lock screen wake inside `wait`.
    pub(crate) lua: Option<Lua>,
    pub(crate) app: Rc<AppFiles>,
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
    /// The fonts of every canvas, `ctx.fonts` and the plates.
    pub(crate) fonts: Fonts,
    /// `ctx.image`, `ctx.qr` and `ctx.layers` images by `_id`, until released; dropped with the Lua state.
    pub(crate) images: HashMap<i64, Rc<Gray>>,
    pub(crate) next_image: i64,
    pub(crate) frame_commands: usize,
    /// `g:image` transform options were already warned about in this session.
    pub(crate) transform_warned: bool,
    /// The API the running package declares: §7 text options are read from 2.16 and 2.28 on.
    pub(crate) app_api: (u32, u32),
    pub(crate) locked: bool,
    pub(crate) lock_wakes: u32,
    pub(crate) lock: Lock,
    pub(crate) frontlight: i64,
    /// `clock:now().revision`: counts the host's moves of the clock.
    pub(crate) clock_revision: i32,
    /// For `ctx.random`; lives as long as the core, so uuids never repeat across restarts.
    pub(crate) rng: ChaCha20Rng,
    pub(crate) persisted: Persisted,
    pub(crate) persist_changed: bool,
    pub(crate) net: NetState,
    /// Moves when a new frame is drawn; see `Core::frame_seq`.
    pub(crate) frame_seq: u64,
    pub(crate) plates_sig: PlatesSig,
    /// The next frame is a full refresh: the session's first, `invalidate("full")`, `act("refresh")`.
    pub(crate) full_next: bool,
    /// Partial frames since the last full one.
    pub(crate) partials: u32,
    /// Clock ms up to which `defer_full_refresh` holds back the automatic full refresh.
    pub(crate) defer_until: i64,
    pub(crate) refreshes: VecDeque<Refresh>,
    pub(crate) perf: Perf,
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
        self.full_next = false;
        self.net.stop();
    }

    /// Records a finished app frame: its refresh kind and its `perf` counters.
    fn presented(&mut self, forced_full: bool) {
        let next = self.partials + 1;
        let deferred = self.sched.clock.ms < self.defer_until;
        // a deferral cannot hold the automatic full past twice the interval (2.28.1)
        let auto = next >= AUTO_FULL_EVERY && (!deferred || next >= 2 * AUTO_FULL_EVERY);
        let kind = if forced_full || auto {
            Refresh::Full
        } else {
            Refresh::Partial
        };
        self.partials = if kind == Refresh::Partial { next } else { 0 };
        if self.refreshes.len() == KEPT_REFRESHES {
            self.refreshes.pop_front();
        }
        self.refreshes.push_back(kind);
        let (cmd_count, image_count) = self.canvas.accepted();
        self.perf = Perf {
            cmd_count,
            image_count,
            draw_frame: self.perf.draw_frame + 1,
        };
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

/// The current Lua state.
pub(crate) fn current_lua(st: &Shared) -> Lua {
    with(st, |s| s.lua.clone()).expect("the core has no Lua state")
}

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
                    // a lock screen wake boots a new Lua state: every step runs in the current one
                    Host::with_current(self.st, |h| payload(h));
                    Host::with_current(self.st, |h| h.settle());
                    with(self.st, |s| s.sched.rearm(id));
                }
            }
        }
        with(self.st, |s| s.sched.advance_to(target));
        Host::with_current(self.st, |h| h.settle());
        Ok(())
    }

    /// Runs `f` with a host over the current Lua state.
    pub fn with_current<R>(st: &Shared, f: impl FnOnce(&Host) -> R) -> R {
        let lua = current_lua(st);
        f(&Host { lua: &lua, st })
    }

    pub fn leave(&self) {
        if with(self.st, |s| s.locked) {
            lockscreen::end_lock(self.st);
            with(self.st, |s| {
                s.log_line("left while locked: the lock screen was dropped")
            });
            return;
        }
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
        if let Some(ready) = with(self.st, |s| {
            s.lock.waiting.then(|| s.lock.ready(s.net.inflight))
        }) {
            if ready {
                lockscreen::present(self);
            }
            return;
        }
        let draw = with(self.st, |s| std::mem::take(&mut s.invalid) && s.running);
        if draw {
            self.draw();
        }
    }

    fn draw(&self) {
        // taken before on_draw: a full request made while drawing is for the next frame
        let (ctx, g, full, was) = with(self.st, |s| {
            s.canvas.begin_frame();
            let was = s.canvas.set_active(true);
            (
                s.ctx.clone(),
                s.g.clone(),
                std::mem::take(&mut s.full_next),
                was,
            )
        });
        self.call("on_draw", (ctx, g));
        with(self.st, |s| {
            s.canvas.set_active(was);
            s.frame_seq += 1;
            let n = s.canvas.end_frame();
            for w in s.canvas.take_warnings() {
                s.warn(&w);
            }
            match s.canvas.take_bad() {
                Some(bad) => {
                    s.fail(&format!("g: bad values in on_draw: {bad}"), ERROR_TITLE);
                    s.canvas.begin_frame();
                }
                None => {
                    s.frame_commands = n;
                    if s.running {
                        s.presented(full);
                    }
                }
            }
        });
    }
}

/// One emulated InkApp.
pub struct Core {
    st: Shared,
}

impl Drop for Core {
    fn drop(&mut self) {
        // the Lua state's functions hold the shared state: break the cycle, dropping it outside any borrow
        let lua = with(&self.st, |s| s.lua.take());
        drop(lua);
    }
}

/// How [`boot`] starts the Lua state.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum BootMode {
    App,
    /// The app after the lock screen: `on_load`, then `on_resume`.
    Resume,
    /// The manifest's lock screen entry.
    Lock,
}

/// 0.1 `_boot`: a fresh Lua state for the app or its lock screen: manifest, ctx, entry chunk, `on_load`
/// (+ `on_resume`), first frame. Never called with a borrow held.
pub(crate) fn boot(st: &Shared, mode: BootMode) {
    let (fw, label, budget, app) = with(st, |s| {
        s.stop();
        (s.overlay, s.ctx, s.g, s.quit, s.toast) = (None, None, None, false, None);
        (s.images, s.next_image) = (HashMap::new(), 0);
        s.net.inflight = 0;
        s.lock.waiting = false;
        (s.full_next, s.partials, s.defer_until, s.perf) = (true, 0, 0, Perf::default());
        if mode != BootMode::Lock {
            s.lock.ctx = None;
        }
        (
            s.cfg.firmware_api,
            s.cfg.app_dir_label.clone(),
            s.cfg.instruction_budget,
            s.app.clone(),
        )
    });
    let (manifest, problem) = read_manifest(&app, fw, &label);
    let landscape = manifest
        .get("display")
        .and_then(|d| d.get("orientation"))
        .is_some_and(|o| o == "landscape");
    let new = lua::new_lua();
    lua::set_budget(&new, budget);
    // the state keeps the original handle: dropping that one runs a full collection, a clone does not
    let lua = new.clone();
    let old = with(st, |s| {
        // a timer wake keeps showing the previous frame until the new state draws (e-ink keeps its image)
        s.lock.stale =
            mode == BootMode::Lock && s.lock.ctx.as_ref().is_some_and(|c| c.reason == "timer");
        if !s.lock.stale {
            s.canvas = if landscape {
                Canvas::new(800, 480, s.fonts.clone())
            } else {
                Canvas::new(480, 800, s.fonts.clone())
            };
        }
        s.frame_commands = 0;
        s.transform_warned = false;
        s.app_api = api_of(&manifest);
        s.frame_seq += 1;
        s.lua.replace(new)
    });
    drop(old);
    if let Some(p) = problem {
        with(st, |s| s.fail(&p, INSTALL_FAILED));
        return;
    }
    let entry = if mode == BootMode::Lock {
        let Some(block) = lockscreen::block(&manifest) else {
            return;
        };
        match block.get("entry") {
            Some(e) if lockscreen::truthy(e) => e.clone(),
            _ => "lockscreen.lua".into(),
        }
    } else {
        manifest
            .get("entry")
            .cloned()
            .unwrap_or_else(|| "index.lua".into())
    };
    let Some((found, src)) = entry.as_str().and_then(|e| app.find(e)) else {
        let msg = format!(
            "manifest.json: entry {} not found in the app folder",
            py_repr(&entry)
        );
        with(st, |s| s.fail(&msg, INSTALL_FAILED));
        return;
    };
    let permitted = |name: &str| {
        manifest
            .get("permissions")
            .and_then(|p| p.as_array())
            .is_some_and(|p| p.iter().any(|v| v == name))
    };
    // 0.1 `_net_enabled`: the lock screen has net when its manifest block says so
    let net = with(st, |s| s.lock.ctx.as_ref().map(|c| c.net)).unwrap_or_else(|| permitted("net"));
    let lockscreen_api = manifest.get("lockscreen").is_some_and(|b| b.is_object());
    let (ctx, g) = install(&lua, st, &app, permitted("reader"), net, lockscreen_api)
        .expect("cannot set up the app's Lua state");
    with(st, |s| (s.ctx, s.g) = (Some(ctx.clone()), Some(g)));
    let f = match lua::load_chunk(&lua, src, &format!("@{found}")) {
        Ok(f) => f,
        Err(e) => {
            with(st, |s| s.fail(&format!("{found}: {e}"), ERROR_TITLE));
            return;
        }
    };
    with(st, |s| {
        s.running = true;
        s.invalid = true;
        let msg = format!("app {} started", s.app_id);
        s.log_line(&msg);
    });
    let h = Host { lua: &lua, st };
    h.protected(&f, (), &found);
    h.call("on_load", ctx.clone());
    if mode == BootMode::Resume {
        h.call("on_resume", ctx);
    }
    if mode == BootMode::Lock {
        lockscreen::after_load(&h);
    } else {
        h.settle();
    }
}

/// `require`, the global `ctx` and the `g` passed to `on_draw`.
fn install(
    lua: &Lua,
    st: &Shared,
    app: &Rc<AppFiles>,
    reader: bool,
    net: bool,
    lockscreen_api: bool,
) -> mlua::Result<(Table, Table)> {
    lua::install_require(lua, app.clone())?;
    let ctx = crate::ctx::new_ctx(lua, st, app, reader, net, lockscreen_api)?;
    lua.globals().set("ctx", &ctx)?;
    let g = crate::draw::new_g(lua, st, Target::Screen)?;
    Ok((ctx, g))
}

impl Core {
    /// Boots the app: manifest, fresh Lua state, entry chunk, `on_load`, first frame.
    pub fn new(app: AppFiles, cfg: Config, persisted: Persisted) -> Core {
        Core::with_fetcher(app, cfg, persisted, None)
    }

    /// Like [`Core::new`], with the fetcher that performs `record`/`live` requests from the first boot on.
    pub fn with_fetcher(
        app: AppFiles,
        mut cfg: Config,
        persisted: Persisted,
        fetcher: Option<Fetcher>,
    ) -> Core {
        let app_id = app_id_of(
            &read_manifest(&app, cfg.firmware_api, &cfg.app_dir_label).0,
            &cfg.fallback_app_id,
        );
        let clock = VirtualClock::new(cfg.start_local_ms);
        let mut net = NetState::new(
            FixtureSet::from_files(std::mem::take(&mut cfg.fixtures)),
            Redactor::new(&cfg.secrets),
        );
        net.fetcher = fetcher;
        let (fonts, font_problem) = match cfg.fonts.take().as_ref().map(Fonts::from_bdf) {
            None => (Fonts::new(), None),
            Some(Ok(f)) => (f, None),
            Some(Err(e)) => (
                Fonts::new(),
                Some(format!("fonts: {e}; drawing with DejaVu Sans")),
            ),
        };
        let rng = match cfg.random_seed {
            Some(seed) => ChaCha20Rng::seed_from_u64(seed),
            None => {
                let mut seed = [0u8; 32];
                getrandom::fill(&mut seed).expect("no OS entropy for ctx.random");
                ChaCha20Rng::from_seed(seed)
            }
        };
        let st = Rc::new(RefCell::new(State {
            lua: None,
            app: Rc::new(app),
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
            canvas: Canvas::new(480, 800, fonts.clone()),
            fonts,
            images: HashMap::new(),
            next_image: 0,
            frame_commands: 0,
            transform_warned: false,
            app_api: (0, 8),
            locked: false,
            lock_wakes: 0,
            lock: Lock::default(),
            frontlight: 0,
            clock_revision: 1,
            rng,
            persisted,
            persist_changed: false,
            net,
            frame_seq: 0,
            plates_sig: PlatesSig::default(),
            full_next: false,
            partials: 0,
            defer_until: 0,
            refreshes: VecDeque::new(),
            perf: Perf::default(),
        }));
        if let Some(p) = font_problem {
            st.borrow_mut().warn(&p);
        }
        boot(&st, BootMode::App);
        Core { st }
    }

    /// Runs `f` with a host over the current Lua state.
    fn run<R>(&self, f: impl FnOnce(&Host) -> R) -> R {
        Host::with_current(&self.st, f)
    }

    // ---- input --------------------------------------------------------------------------------

    /// Low-level pointer event; `up` classifies the gesture and advances 350 ms.
    pub fn pointer(&mut self, phase: &str, x: i32, y: i32) -> Result<(), String> {
        self.run(|h| h.pointer(phase, x, y))
    }

    pub fn tap(&mut self, x: i32, y: i32) -> Result<(), String> {
        self.run(|h| h.tap(x, y))
    }

    pub fn double_tap(&mut self, x: i32, y: i32) -> Result<(), String> {
        self.run(|h| h.double_tap(x, y))
    }

    pub fn hold(&mut self, x: i32, y: i32, ms: i64) -> Result<(), String> {
        self.run(|h| h.hold(x, y, ms))
    }

    pub fn drag(&mut self, points: &[(i32, i32)], duration_ms: i64) -> Result<(), String> {
        self.run(|h| h.drag(points, duration_ms))
    }

    pub fn swipe(&mut self, dir: &str, at: (i32, i32), distance: i32) -> Result<(), String> {
        self.run(|h| h.swipe(dir, at, distance))
    }

    /// Taps the last drawn text equal to `text`, else the last containing it.
    pub fn tap_text(&mut self, text: &str) -> Result<(), String> {
        self.run(|h| h.tap_text(text))
    }

    pub fn key(&mut self, name: &str) -> Result<(), String> {
        self.run(|h| h.key(name))
    }

    /// Answers a pending `input:text` with OK, or types into a pending `input:edit` and presses the confirm key
    /// (a newline is the confirm key too; with `enter = "newline"` it inserts a line break and the edit stays open).
    pub fn type_text(&mut self, text: &str) -> Result<(), String> {
        self.run(|h| h.type_text(text))
    }

    /// Submits a front end's text field: answers a pending `input:text` with `text`, or replaces the text of a
    /// pending `input:edit` with it and presses the confirm key.
    pub fn submit_text(&mut self, text: &str) -> Result<(), String> {
        self.run(|h| h.submit_text(text))
    }

    /// Answers the pending dialog: confirm yes/no, leave yes/no, text OK/cancel, edit confirm key/Back.
    pub fn answer(&mut self, ok: bool) -> Result<(), String> {
        self.run(|h| h.answer(ok))
    }

    // ---- network ------------------------------------------------------------------------------

    /// Sets the fetcher that performs `record`/`live` requests at request time; None leaves them in the outbox.
    pub fn set_fetcher(&mut self, fetcher: Option<Fetcher>) {
        let old = with(&self.st, |s| std::mem::replace(&mut s.net.fetcher, fetcher));
        drop(old);
    }

    /// `record`/`live` requests made since the last call, for a host without a fetcher to perform.
    pub fn take_outgoing(&mut self) -> Vec<OutRequest> {
        with(&self.st, |s| s.net.take_outgoing())
    }

    /// The host's answer to an outgoing request; the callback fires at request time + delay, at once if that
    /// has passed.
    pub fn deliver(&mut self, id: u64, result: NetResult) {
        self.run(|h| net::deliver(h, id, result));
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

    /// Jumps virtual time forward, firing due timers and redrawing as needed; `clock:now().revision` changes.
    pub fn wait(&mut self, ms: i64) -> Result<(), String> {
        if ms > 0 {
            with(&self.st, |s| s.clock_revision += 1);
        }
        self.advance(ms)
    }

    /// Lets virtual time pass as real time does (the window's ticks): like [`Core::wait`], but the clock is not
    /// moved, so `clock:now().revision` stays.
    pub fn advance(&mut self, ms: i64) -> Result<(), String> {
        self.run(|h| h.wait(ms))
    }

    pub fn leave(&mut self) {
        self.run(|h| h.leave());
    }

    pub fn restart(&mut self) {
        self.leave();
        boot(&self.st, BootMode::App);
    }

    pub fn reload(&mut self) {
        if with(&self.st, |s| s.locked) {
            lockscreen::end_lock(&self.st);
        }
        if with(&self.st, |s| s.running) {
            self.run(|h| h.save_state());
        }
        boot(&self.st, BootMode::App);
    }

    /// Replaces the app folder's contents; `reload()` then runs the new code.
    pub fn set_app(&mut self, app: AppFiles) {
        with(&self.st, |s| s.app = Rc::new(app));
    }

    /// Leaves the app, then runs the manifest's lock screen (a blank screen without one).
    pub fn lock(&mut self) {
        self.run(lockscreen::lock);
    }

    /// Drops the lock screen state and starts the app again (`on_load` + `on_resume`, saved state kept).
    pub fn unlock(&mut self) {
        lockscreen::unlock(&self.st);
    }

    pub fn resume(&mut self) {
        with(&self.st, |s| s.paused = false);
    }

    /// The window and the daemon freeze the clock with this; only `status()` reports it.
    pub fn set_paused(&mut self, paused: bool) {
        with(&self.st, |s| s.paused = paused);
    }

    // ---- inspection ----------------------------------------------------------------------------

    pub fn locked(&self) -> bool {
        self.st.borrow().locked
    }

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
            refresh: s.refreshes.back().map(|r| r.as_str()),
            refreshes: s.refreshes.iter().map(|r| r.as_str()).collect(),
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

    /// Changes whenever the next `frame` would differ: an app frame was drawn or the plates changed.
    /// Borrows the state mutably: never call it while a `log()`/`errors()`/... `Ref` is alive.
    pub fn frame_seq(&self) -> u64 {
        let mut s = self.st.borrow_mut();
        let now = s.sched.clock.ms;
        let sig = (
            s.overlay.clone(),
            s.pending.clone(),
            s.toast.as_ref().filter(|t| now < t.1).map(|t| t.0.clone()),
        );
        if sig != s.plates_sig {
            s.plates_sig = sig;
            s.frame_seq += 1;
        }
        s.frame_seq
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
        let lua = current_lua(&self.st);
        let f = lua::load_chunk(&lua, code.as_bytes(), "=lua_exec")?;
        lua::call_guarded(&lua, &f, ()).map(|_| ())
    }

    /// Runs Lua code in the app's state under the budget and returns its first result as JSON (test helper).
    pub fn lua_eval(&mut self, code: &str) -> Result<serde_json::Value, String> {
        let lua = current_lua(&self.st);
        let f = lua::load_chunk(&lua, code.as_bytes(), "=lua_eval")?;
        let r = lua::call_guarded(&lua, &f, ())?;
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

    fn lock_app() -> AppFiles {
        let manifest = br#"{"app_id": "testapp", "entry": "index.lua", "api": "2.2",
            "lockscreen": {"interval_sec": 60}}"#;
        let lock = b"function on_load(c) c:after(5000, function() end) end\n\
            function on_draw(c, g) g:text(0, 0, 'locked') end\n";
        AppFiles(
            [
                ("manifest.json".to_string(), manifest.to_vec()),
                (
                    "index.lua".to_string(),
                    b"function on_draw(c, g) end\n".to_vec(),
                ),
                ("lockscreen.lua".to_string(), lock.to_vec()),
            ]
            .into(),
        )
    }

    /// The Lua state's functions hold the shared state; dropping the core must free both.
    #[test]
    fn dropping_the_core_frees_its_state() {
        let core = Core::new(lock_app(), Config::default(), Persisted::default());
        let weak = Rc::downgrade(&core.st);
        drop(core);
        assert!(weak.upgrade().is_none());

        let mut core = Core::new(lock_app(), Config::default(), Persisted::default());
        core.lock();
        core.wait(61_000).unwrap();
        assert_eq!(core.lock_wakes(), 1);
        let weak = Rc::downgrade(&core.st);
        drop(core);
        assert!(weak.upgrade().is_none());
    }

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
        let lua = current_lua(&core.st);
        let host_wait = lua
            .create_function(move |lua, ms: i64| {
                Host { lua, st: &st }.wait(ms).map_err(mlua::Error::runtime)
            })
            .unwrap();
        lua.globals().set("host_wait", host_wait).unwrap();
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
