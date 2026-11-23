//! Pointer and key input with gesture classification, as 0.1 `Emu.pointer`/`tap`/.../`_send`/`_dispatch`.
use std::rc::Rc;

use mlua::{Lua, Table, Value};

use crate::dialog::Handler;
use crate::emu::{with, Host};
use crate::log::py_repr_str;

/// Every input advances virtual time by this much.
pub const INPUT_MS: i64 = 350;
pub const HOLD_MS: i64 = 600;
const KEYS: [&str; 6] = ["up", "down", "left", "right", "ok", "back"];
const PHASES: [&str; 4] = ["down", "move", "hold", "up"];
const SWIPES: [(&str, (i64, i64)); 4] = [
    ("left", (-1, 0)),
    ("right", (1, 0)),
    ("up", (0, -1)),
    ("down", (0, 1)),
];

/// Gesture on pointer up; a 20..60 px move that is not a hold has no gesture.
pub fn classify(dx: i32, dy: i32, held_ms: i64) -> Option<&'static str> {
    let (ax, ay) = (dx.unsigned_abs(), dy.unsigned_abs());
    if ax.max(ay) >= 60 {
        return Some(if ax >= ay {
            if dx > 0 {
                "swipe_right"
            } else {
                "swipe_left"
            }
        } else if dy > 0 {
            "swipe_down"
        } else {
            "swipe_up"
        });
    }
    if held_ms >= HOLD_MS {
        return Some("long");
    }
    (ax.max(ay) < 20).then_some("tap")
}

/// One input event as the app's `on_input` sees it.
struct Event {
    kind: &'static str,
    phase: &'static str,
    /// x, y, down_x, down_y
    at: Option<(i32, i32, i32, i32)>,
    key: Option<&'static str>,
    gesture: Option<&'static str>,
    time_ms: i64,
}

impl Event {
    fn key(name: &'static str, phase: &'static str, time_ms: i64) -> Event {
        Event {
            kind: "key",
            phase,
            at: None,
            key: Some(name),
            gesture: None,
            time_ms,
        }
    }

    fn table(&self, lua: &Lua) -> mlua::Result<Table> {
        let t = lua.create_table()?;
        t.raw_set("type", self.kind)?;
        t.raw_set("phase", self.phase)?;
        if let Some((x, y, dx, dy)) = self.at {
            t.raw_set("x", x)?;
            t.raw_set("y", y)?;
            t.raw_set("down_x", dx)?;
            t.raw_set("down_y", dy)?;
        }
        t.raw_set("key", self.key)?;
        t.raw_set("gesture", self.gesture)?;
        let ms = match i32::try_from(self.time_ms) {
            Ok(i) => Value::Integer(i),
            Err(_) => Value::Number(self.time_ms as _),
        };
        t.raw_set("time_ms", ms)?;
        Ok(t)
    }
}

enum Gate {
    Go,
    Dialog(&'static str),
    Ignored(&'static str),
}

impl Host<'_> {
    fn now_ms(&self) -> i64 {
        with(self.st, |s| s.sched.clock.ms)
    }

    /// Delivers one event to `on_input` and redraws if needed, without advancing time.
    fn send(&self, ev: &Event) {
        let gate = with(self.st, |s| {
            if s.locked {
                Gate::Ignored("the screen is locked")
            } else if let Some(p) = &s.pending {
                Gate::Dialog(p.kind)
            } else if !s.running {
                Gate::Ignored("the app is not running")
            } else {
                Gate::Go
            }
        });
        match gate {
            Gate::Ignored(why) => {
                let msg = format!("input ignored, {why}: {} {}", ev.kind, ev.phase);
                return with(self.st, |s| s.log_line(&msg));
            }
            Gate::Dialog("edit") if ev.key == Some("back") => {
                if ev.phase == "up" {
                    with(self.st, |s| s.log_line("back ended the edit"));
                    let _ = self.answer_now(false);
                }
                return;
            }
            Gate::Dialog(kind) => {
                if ev.phase == "up" {
                    let how = if matches!(kind, "text" | "edit") {
                        "type text or answer it"
                    } else {
                        "answer it"
                    };
                    let what = ev.gesture.or(ev.key).unwrap_or_default();
                    let msg = format!(
                        "input went to the open {kind} dialog, not the app: {} {what} ({how})",
                        ev.kind
                    );
                    with(self.st, |s| s.log_line(&msg));
                }
                return;
            }
            Gate::Go => {}
        }
        let Ok(t) = ev.table(self.lua) else { return };
        let handled = self
            .call("on_input", (self.ctx(), t))
            .is_some_and(|v| crate::lua::truthy(&v));
        let offer = ev.key == Some("back")
            && !handled
            && with(self.st, |s| s.running && s.pending.is_none());
        if offer {
            let leave = Handler {
                answer: Rc::new(|h: &Host, ok| {
                    if ok {
                        h.leave();
                    }
                }),
                type_text: None,
                ok_keeps_open: false,
            };
            self.set_pending("leave", "Leave app?".into(), String::new(), leave);
            with(self.st, |s| {
                s.log_line("back not handled by on_input: the host offers to leave")
            });
        }
        self.settle();
    }

    fn dispatch(&self, ev: &Event) -> Result<(), String> {
        self.send(ev);
        self.wait(INPUT_MS)
    }

    pub fn pointer(&self, phase: &str, x: i32, y: i32) -> Result<(), String> {
        let Some(&phase) = PHASES.iter().find(|p| **p == phase) else {
            return Err(format!(
                "unknown pointer phase {}: use one of {}",
                py_repr_str(phase),
                PHASES.join(", ")
            ));
        };
        let now = self.now_ms();
        let down = with(self.st, |s| {
            if phase == "down" {
                s.down = Some((x, y, now));
                s.hold_sent = false;
            } else if s.down.is_none() {
                s.log_line(&format!("pointer {phase} without down ignored"));
                return None;
            }
            s.hold_sent |= phase == "hold";
            let d = s.down?;
            if phase == "up" {
                s.down = None;
            }
            Some((d, s.hold_sent))
        });
        let Some(((dx, dy, t0), hold_sent)) = down else {
            return Ok(());
        };
        let mut ev = Event {
            kind: "pointer",
            phase,
            at: Some((x, y, dx, dy)),
            key: None,
            gesture: None,
            time_ms: now,
        };
        if phase != "up" {
            self.send(&ev);
            return Ok(());
        }
        // a frozen window clock must not turn a delivered hold into a tap
        let held = (now - t0).max(if hold_sent { HOLD_MS } else { 0 });
        ev.gesture = classify(x.saturating_sub(dx), y.saturating_sub(dy), held);
        if ev.gesture == Some("tap") {
            if let Some((label, ok)) = self.dialog_hit(x, y) {
                with(self.st, |s| {
                    let kind = s.pending.as_ref().map_or("", |p| p.kind);
                    let msg = format!("{kind} dialog: {label} tapped");
                    s.log_line(&msg);
                });
                // a lock screen wake in an earlier wait of this gesture may have replaced the Lua state
                return Host::with_current(self.st, |h| h.answer(ok));
            }
        }
        self.dispatch(&ev)
    }

    pub fn tap(&self, x: i32, y: i32) -> Result<(), String> {
        self.pointer("down", x, y)?;
        self.pointer("up", x, y)
    }

    pub fn double_tap(&self, x: i32, y: i32) -> Result<(), String> {
        self.tap(x, y)?;
        self.wait(100)?;
        self.pointer("down", x, y)?;
        with(self.st, |s| s.down = None);
        let ev = Event {
            kind: "pointer",
            phase: "up",
            at: Some((x, y, x, y)),
            key: None,
            gesture: Some("double_tap"),
            time_ms: self.now_ms(),
        };
        self.dispatch(&ev)
    }

    pub fn hold(&self, x: i32, y: i32, mut ms: i64) -> Result<(), String> {
        self.pointer("down", x, y)?;
        if ms >= HOLD_MS {
            self.wait(HOLD_MS)?;
            self.pointer("hold", x, y)?;
            ms -= HOLD_MS;
        }
        self.wait(ms)?;
        self.pointer("up", x, y)
    }

    pub fn drag(&self, points: &[(i32, i32)], duration_ms: i64) -> Result<(), String> {
        let [(x0, y0), rest @ ..] = points else {
            return Err("drag() needs at least two points".into());
        };
        let Some(&(xn, yn)) = rest.last() else {
            return Err("drag() needs at least two points".into());
        };
        self.pointer("down", *x0, *y0)?;
        let step = duration_ms.max(0) / rest.len() as i64;
        for &(x, y) in rest {
            self.wait(step)?;
            self.pointer("move", x, y)?;
        }
        self.pointer("up", xn, yn)
    }

    pub fn swipe(&self, dir: &str, at: (i32, i32), distance: i32) -> Result<(), String> {
        let Some(&(_, (ux, uy))) = SWIPES.iter().find(|(d, _)| *d == dir) else {
            let names: Vec<_> = SWIPES.iter().map(|(d, _)| *d).collect();
            return Err(format!(
                "unknown swipe direction {}: use one of {}",
                py_repr_str(dir),
                names.join(", ")
            ));
        };
        const STEPS: i64 = 5;
        let clamp = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let points: Vec<(i32, i32)> = (0..=STEPS)
            .map(|i| {
                let d = i64::from(distance) * i;
                (
                    clamp(i64::from(at.0) + (ux * d).div_euclid(STEPS)),
                    clamp(i64::from(at.1) + (uy * d).div_euclid(STEPS)),
                )
            })
            .collect();
        self.drag(&points, 300)
    }

    pub fn tap_text(&self, text: &str) -> Result<(), String> {
        let items = with(self.st, |s| s.texts());
        let exact = items.iter().rev().find(|it| it.text == text);
        let hit = exact.or_else(|| items.iter().rev().find(|it| it.text.contains(text)));
        let Some(it) = hit else {
            let visible: Vec<String> = items.iter().map(|it| py_repr_str(&it.text)).collect();
            return Err(format!(
                "no text {} on screen; visible: [{}]",
                py_repr_str(text),
                visible.join(", ")
            ));
        };
        self.tap(
            it.x.saturating_add(it.w.div_euclid(2)),
            it.y.saturating_add(it.h.div_euclid(2)),
        )
    }

    pub fn key(&self, name: &str) -> Result<(), String> {
        let Some(&name) = KEYS.iter().find(|k| **k == name) else {
            return Err(format!(
                "unknown key {}: use one of {}",
                py_repr_str(name),
                KEYS.join(", ")
            ));
        };
        self.send(&Event::key(name, "down", self.now_ms()));
        self.dispatch(&Event::key(name, "up", self.now_ms()))
    }
}

#[cfg(test)]
mod tests {
    use super::classify;

    #[test]
    fn classify_as_0_1() {
        assert_eq!(classify(0, 0, 0), Some("tap"));
        assert_eq!(classify(19, -19, 599), Some("tap"));
        assert_eq!(classify(20, 0, 0), None);
        assert_eq!(classify(59, 0, 0), None);
        assert_eq!(classify(30, 0, 600), Some("long"));
        assert_eq!(classify(60, 0, 0), Some("swipe_right"));
        assert_eq!(classify(-60, 60, 0), Some("swipe_left"));
        assert_eq!(classify(10, 61, 9999), Some("swipe_down"));
        assert_eq!(classify(0, -60, 0), Some("swipe_up"));
        assert_eq!(classify(i32::MIN, 0, 0), Some("swipe_left"));
    }
}
