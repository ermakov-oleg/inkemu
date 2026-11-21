//! The lock screen (0.1 `emu.py` lock section): `lock`/`unlock`, the manifest's `lockscreen` block, a separate
//! Lua state for its entry, wakes every `interval_sec`, the 30 s network wait and the stale frame on timer wakes.
use std::rc::Rc;

use serde_json::{Map, Value as J};

use crate::app::read_manifest;
use crate::clock::TimerId;
use crate::draw::Canvas;
use crate::emu::{boot, with, BootMode, Host, Shared, State};

/// How long a timer wake waits for the lock screen's requests before it shows the frame anyway.
pub(crate) const LOCK_WAIT_MS: i64 = 30_000;

/// A number as the app or the manifest gave it: Lua shows `120` and `120.0` differently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Num {
    Int(i64),
    Float(f64),
}

impl Default for Num {
    fn default() -> Self {
        Num::Int(0)
    }
}

impl Num {
    pub(crate) fn positive(self) -> bool {
        match self {
            Num::Int(i) => i > 0,
            Num::Float(f) => f > 0.0,
        }
    }

    /// Python `int(sec * 1000)`.
    fn ms(self) -> i64 {
        match self {
            Num::Int(i) => i.saturating_mul(1000),
            Num::Float(f) => (f * 1000.0) as i64,
        }
    }
}

/// 0.1 `_lock_ctx`: why the running lock screen state was started.
pub(crate) struct LockCtx {
    /// "lock" or "timer".
    pub(crate) reason: &'static str,
    pub(crate) net: bool,
    pub(crate) done: bool,
    pub(crate) elapsed_sec: i64,
}

/// The lock screen's part of the core state.
#[derive(Default)]
pub(crate) struct Lock {
    /// Set while the lock screen entry runs.
    pub(crate) ctx: Option<LockCtx>,
    pub(crate) interval: Num,
    /// Clock ms of the last lock or wake.
    pub(crate) cycle_ms: i64,
    /// A timer wake waits for its requests (or `ctx.lock:done()`) before drawing.
    pub(crate) waiting: bool,
    pub(crate) wake_timer: Option<TimerId>,
    pub(crate) deadline: Option<TimerId>,
    /// The previous frame stays on screen until the woken state draws.
    pub(crate) stale: bool,
}

impl Lock {
    pub(crate) fn ready(&self, inflight: u32) -> bool {
        self.ctx.as_ref().is_some_and(|c| c.done) || inflight == 0
    }
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

/// 0.1 `int(_lock_block_get("interval_sec", 0))`: a positive number (not a boolean), else 0.
fn interval_sec(manifest: &Map<String, J>) -> i64 {
    match block(manifest).and_then(|b| b.get("interval_sec")) {
        Some(J::Number(n)) if n.as_f64().is_some_and(|f| f > 0.0) => n
            .as_i64()
            .unwrap_or_else(|| n.as_f64().unwrap_or_default() as i64),
        _ => 0,
    }
}

/// Leaves the app, then runs the manifest's lock screen (a blank screen without one).
pub(crate) fn lock(h: &Host) {
    if with(h.st, |s| s.locked) {
        return;
    }
    h.leave();
    let iv = interval_sec(&manifest(h.st));
    with(h.st, |s| {
        s.locked = true;
        s.lock_wakes = 0;
        s.lock.interval = Num::Int(iv);
    });
    enter(h.st, "lock");
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
        s.lock.waiting = false;
        s.lock.stale = false;
        s.lock.wake_timer = None;
    });
}

/// 0.1 `_lock_enter`: boots the lock screen entry in a fresh Lua state and schedules the next wake.
fn enter(st: &Shared, reason: &'static str) {
    let net = block(&manifest(st))
        .and_then(|b| b.get("net"))
        .is_some_and(truthy);
    with(st, |s| {
        let now = s.sched.clock.ms;
        let elapsed_sec = if reason == "timer" {
            now.saturating_sub(s.lock.cycle_ms).div_euclid(1000)
        } else {
            0
        };
        s.lock.ctx = Some(LockCtx {
            reason,
            net,
            done: false,
            elapsed_sec,
        });
        s.lock.cycle_ms = now;
    });
    boot(st, BootMode::Lock);
    with(st, schedule);
}

/// 0.1 `_lock_schedule`: the next wake, `interval` after the last lock or wake.
pub(crate) fn schedule(s: &mut State) {
    if let Some(t) = s.lock.wake_timer.take() {
        s.cancel_timer(t);
    }
    if s.locked && s.running && s.lock.interval.positive() {
        let due = s
            .lock
            .cycle_ms
            .saturating_add(s.lock.interval.ms())
            .saturating_sub(s.sched.clock.ms);
        s.lock.wake_timer = Some(s.add_timer(due, Rc::new(wake), 0));
    }
}

fn wake(h: &Host) {
    with(h.st, |s| {
        s.lock_wakes += 1;
        s.log_line("lock screen wake");
    });
    enter(h.st, "timer");
}

/// 0.1 `_lock_after_load`: a lock draws at once; a timer wake first waits for its requests, at most 30 s.
pub(crate) fn after_load(h: &Host) {
    let now = with(h.st, |s| {
        if !s.running {
            return false;
        }
        let by_lock = s.lock.ctx.as_ref().is_some_and(|c| c.reason == "lock");
        if by_lock || s.lock.ready(s.net.inflight) {
            return true;
        }
        s.lock.waiting = true;
        s.lock.deadline = Some(s.add_timer(LOCK_WAIT_MS, Rc::new(present), 0));
        false
    });
    if now {
        present(h);
    }
}

/// 0.1 `_lock_present`: ends the wait and draws the lock screen on a fresh canvas.
pub(crate) fn present(h: &Host) {
    with(h.st, |s| {
        if std::mem::take(&mut s.lock.waiting) {
            if let Some(t) = s.lock.deadline.take() {
                s.cancel_timer(t);
            }
        }
        if std::mem::take(&mut s.lock.stale) {
            s.canvas = Canvas::new(s.canvas.w, s.canvas.h);
        }
        s.invalid = true;
    });
    h.settle();
}
