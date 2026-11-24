//! The lock screen (§15): `lock`/`unlock`, the manifest's `lockscreen` block, one short session per lock and per
//! wake in a fresh Lua state, wakes aligned to the interval, and the preparation phase of a networked session.
use std::rc::Rc;

use serde_json::{Map, Value as J};

use crate::app::read_manifest;
use crate::clock::TimerId;
use crate::draw::Canvas;
use crate::emu::{boot, with, BootMode, Host, Shared, State};

/// The preparation phase of a networked session ends this long after the session started.
pub(crate) const PREP_MS: i64 = 28_000;
const INTERVAL_DEFAULT: i64 = 60;
const INTERVAL_MIN: i64 = 30;
const INTERVAL_MAX: i64 = 3600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// `on_load` runs; nothing is presented yet.
    Load,
    /// A networked session waits for its requests and timers.
    Prep,
    /// The session's frame is presented; no callback runs any more.
    Ended,
}

/// The running lock-screen session.
pub(crate) struct LockCtx {
    /// "first" or "timer".
    pub(crate) reason: &'static str,
    /// The manifest declares `lockscreen.net`: `ctx:after` works and the session can wait for it.
    pub(crate) declared: bool,
    /// `ctx.lock.net`.
    pub(crate) net: bool,
    pub(crate) done: bool,
    pub(crate) elapsed_sec: i64,
    pub(crate) phase: Phase,
    /// `on_draw` runs: `lock:done()` has no effect there.
    pub(crate) drawing: bool,
    /// The session's `ctx:after` timers, which the preparation phase waits for.
    pub(crate) afters: Vec<TimerId>,
}

/// The lock screen's part of the core state.
#[derive(Default)]
pub(crate) struct Lock {
    /// Set while the lock screen is up.
    pub(crate) ctx: Option<LockCtx>,
    pub(crate) interval: i64,
    pub(crate) net_interval: i64,
    /// Clock ms when the running session started.
    pub(crate) start_ms: i64,
    /// Clock ms of the last lock-screen present.
    pub(crate) presented_ms: i64,
    /// Clock ms when the last session with `lock.net` started.
    pub(crate) net_ms: Option<i64>,
    pub(crate) wake_timer: Option<TimerId>,
    pub(crate) deadline: Option<TimerId>,
    /// The previous frame stays on screen until the woken session draws.
    pub(crate) stale: bool,
}

/// The preparation phase may end: `lock:done()`, or no request and no `ctx:after` timer left.
pub(crate) fn ready(s: &State) -> bool {
    s.lock.ctx.as_ref().is_some_and(|c| {
        c.done || (s.net.inflight == 0 && !c.afters.iter().any(|t| s.timers.contains(t)))
    })
}

/// Python truthiness of a JSON value.
pub(crate) fn truthy(v: &J) -> bool {
    match v {
        J::Null => false,
        J::Bool(b) => *b,
        J::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        J::String(s) => !s.is_empty(),
        J::Array(a) => !a.is_empty(),
        J::Object(o) => !o.is_empty(),
    }
}

/// 0.1 `_lock_block`: the manifest's `lockscreen` object unless it says `"enabled": false`.
pub(crate) fn block(manifest: &Map<String, J>) -> Option<&Map<String, J>> {
    let b = manifest.get("lockscreen")?.as_object()?;
    b.get("enabled").is_none_or(truthy).then_some(b)
}

fn manifest(st: &Shared) -> Map<String, J> {
    let (app, fw, label) = with(st, |s| {
        (
            s.app.clone(),
            s.cfg.firmware_api,
            s.cfg.app_dir_label.clone(),
        )
    });
    read_manifest(&app, fw, &label).0
}

/// A manifest interval in seconds, clamped to 30..3600 as at install (§0).
fn seconds(v: Option<&J>) -> Option<i64> {
    let f = v?.as_f64().filter(|f| f.is_finite())?;
    Some((f as i64).clamp(INTERVAL_MIN, INTERVAL_MAX))
}

/// `interval_sec` and `net_interval_sec` of the manifest's block; 0 without a block (no wakes).
fn intervals(manifest: &Map<String, J>) -> (i64, i64) {
    let Some(b) = block(manifest) else {
        return (0, 0);
    };
    let iv = seconds(b.get("interval_sec")).unwrap_or(INTERVAL_DEFAULT);
    (iv, seconds(b.get("net_interval_sec")).unwrap_or(iv))
}

/// Leaves the app, then runs the manifest's lock screen (a blank screen without one).
pub(crate) fn lock(h: &Host) {
    if with(h.st, |s| s.locked) {
        return;
    }
    h.leave();
    let (iv, net_iv) = intervals(&manifest(h.st));
    with(h.st, |s| {
        s.locked = true;
        s.lock_wakes = 0;
        s.lock.interval = iv;
        s.lock.net_interval = net_iv;
        s.lock.net_ms = None;
    });
    enter(h.st, "first");
}

/// Drops the lock screen state and starts the app again (`on_load` + `on_resume`, saved state kept).
pub(crate) fn unlock(st: &Shared) {
    if !with(st, |s| s.locked) {
        return;
    }
    end_lock(st);
    boot(st, BootMode::Resume);
}

/// 0.1 `_end_lock`.
pub(crate) fn end_lock(st: &Shared) {
    with(st, |s| {
        s.stop();
        s.locked = false;
        s.lock.ctx = None;
        s.lock.stale = false;
        s.lock.wake_timer = None;
        s.lock.deadline = None;
    });
}

/// Boots one lock-screen session in a fresh Lua state.
fn enter(st: &Shared, reason: &'static str) {
    let declared = block(&manifest(st))
        .and_then(|b| b.get("net"))
        .is_some_and(truthy);
    with(st, |s| {
        let now = s.sched.clock.ms;
        let first = reason == "first";
        let net = declared
            && (first
                || s.lock.net_ms.is_none_or(|t| {
                    now.saturating_sub(t) >= s.lock.net_interval.saturating_mul(1000)
                }));
        if net {
            s.lock.net_ms = Some(now);
        }
        let elapsed_sec = if first {
            0
        } else {
            now.saturating_sub(s.lock.presented_ms).div_euclid(1000)
        };
        s.lock.ctx = Some(LockCtx {
            reason,
            declared,
            net,
            done: false,
            elapsed_sec,
            phase: Phase::Load,
            drawing: false,
            afters: Vec::new(),
        });
        s.lock.start_ms = now;
    });
    boot(st, BootMode::Lock);
}

/// The next wake: the next whole multiple of the interval in local time.
pub(crate) fn schedule(s: &mut State) {
    if let Some(t) = s.lock.wake_timer.take() {
        s.cancel_timer(t);
    }
    if s.locked && s.running && s.lock.interval > 0 {
        let local = s.cfg.start_local_ms.saturating_add(s.sched.clock.ms);
        let iv = s.lock.interval.saturating_mul(1000);
        let next = (local.div_euclid(iv) + 1).saturating_mul(iv);
        s.lock.wake_timer = Some(s.add_timer(next - local, Rc::new(wake), 0));
    }
}

fn wake(h: &Host) {
    with(h.st, |s| {
        s.lock_wakes += 1;
        s.log_line("lock screen wake");
    });
    enter(h.st, "timer");
}

/// After `on_load`: draws at once, or (a networked session with requests or timers left) enters the preparation
/// phase, drawing the stored data first in a `first` session.
pub(crate) fn after_load(h: &Host) {
    let prep = with(h.st, |s| {
        let c = s.lock.ctx.as_ref()?;
        if !s.running {
            return None;
        }
        Some((c.declared && !ready(s)).then_some(c.reason == "first"))
    });
    match prep {
        None => {}
        Some(None) => present(h),
        Some(Some(early)) => {
            if early {
                draw(h);
            }
            with(h.st, |s| {
                let Some(c) = s.lock.ctx.as_mut().filter(|_| s.running) else {
                    return;
                };
                c.phase = Phase::Prep;
                let due = s
                    .lock
                    .start_ms
                    .saturating_add(PREP_MS)
                    .saturating_sub(s.sched.clock.ms);
                s.lock.deadline = Some(s.add_timer(due, Rc::new(end_prep), 0));
            });
        }
    }
}

fn end_prep(h: &Host) {
    with(h.st, |s| s.lock.deadline = None);
    present(h);
}

/// Unfinished requests and timers never deliver once the session draws its last frame.
fn cancel_all(s: &mut State) {
    for t in std::mem::take(&mut s.timers) {
        s.sched.cancel(t);
    }
    s.tick = None;
    s.lock.deadline = None;
    s.lock.wake_timer = None;
    s.net.stop();
    s.net.inflight = 0;
    if let Some(c) = &mut s.lock.ctx {
        c.afters.clear();
    }
}

/// One `on_draw` of the session, presented.
fn draw(h: &Host) {
    let go = with(h.st, |s| {
        let c = s.lock.ctx.as_mut().filter(|_| s.running)?;
        c.drawing = true;
        Some(())
    });
    if go.is_none() {
        return;
    }
    h.draw();
    with(h.st, |s| {
        if let Some(c) = &mut s.lock.ctx {
            c.drawing = false;
        }
    });
}

/// Ends the session: cancels what is left, draws the frame on a fresh canvas and schedules the next wake.
pub(crate) fn present(h: &Host) {
    let go = with(h.st, |s| {
        let c = s.lock.ctx.as_mut()?;
        if c.phase == Phase::Ended {
            return None;
        }
        c.phase = Phase::Ended;
        cancel_all(s);
        if std::mem::take(&mut s.lock.stale) {
            s.canvas = Canvas::new(s.canvas.w, s.canvas.h, s.fonts.clone());
        }
        Some(())
    });
    if go.is_none() {
        return;
    }
    draw(h);
    with(h.st, |s| {
        cancel_all(s);
        s.lock.presented_ms = s.sched.clock.ms;
        schedule(s);
    });
}
