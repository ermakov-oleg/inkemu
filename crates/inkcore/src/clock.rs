//! Virtual time: a clock that only moves when the host says so, and the timer queue on top of it.
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use crate::log::py_repr_str;

/// Parses `350`, `350ms`, `61s`, `10m` or `2h` (fractions allowed) into milliseconds.
pub fn parse_duration(s: &str) -> Result<i64, String> {
    let bad = || {
        format!(
            "bad duration {}: use 350, 350ms, 61s, 10m or 2h",
            py_repr_str(s)
        )
    };
    let t = s.trim();
    let num_end = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, unit) = (&t[..num_end], t[num_end..].trim_start());
    let (int, frac) = num.split_once('.').unwrap_or((num, "1"));
    if int.is_empty()
        || frac.is_empty()
        || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit())
    {
        return Err(bad());
    }
    let unit_ms = match unit {
        "" | "ms" => 1.0,
        "s" => 1000.0,
        "m" => 60_000.0,
        "h" => 3_600_000.0,
        _ => return Err(bad()),
    };
    let ms = (num.parse::<f64>().map_err(|_| bad())? * unit_ms).round_ties_even();
    if !ms.is_finite() || ms.abs() >= i64::MAX as f64 {
        return Err(bad());
    }
    Ok(ms as i64)
}

/// Milliseconds since 1970-01-01T00:00 for a wall-clock date and time (proleptic Gregorian).
pub fn local_ms(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32, ms: u32) -> i64 {
    let y = i64::from(y) - i64::from(mo <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(mo) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    ((days * 24 + i64::from(h)) * 60 + i64::from(mi)) * 60_000 + i64::from(s) * 1000 + i64::from(ms)
}

/// `(year, month, day, hour, minute, second, millisecond)`.
pub type DateTime = (i32, u32, u32, u32, u32, u32, u32);

/// The inverse of [`local_ms`].
pub fn civil(ms: i64) -> DateTime {
    let (days, ms_of_day) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (yoe + era * 400 + i64::from(mo <= 2)) as i32;
    let (sec, milli) = (ms_of_day / 1000, (ms_of_day % 1000) as u32);
    (
        y,
        mo,
        d,
        (sec / 3600) as u32,
        (sec / 60 % 60) as u32,
        (sec % 60) as u32,
        milli,
    )
}

/// `2026-11-20T09:00:00.350`, as Python's `isoformat(timespec="milliseconds")`.
pub fn iso(t: DateTime) -> String {
    let (y, mo, d, h, mi, s, ms) = t;
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{ms:03}")
}

pub struct VirtualClock {
    start_local_ms: i64,
    /// Milliseconds since the emulator started.
    pub ms: i64,
}

impl VirtualClock {
    pub fn new(start_local_ms: i64) -> Self {
        VirtualClock {
            start_local_ms,
            ms: 0,
        }
    }

    pub fn now(&self) -> DateTime {
        civil(self.start_local_ms + self.ms)
    }

    pub fn advance(&mut self, ms: i64) {
        self.ms += ms;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimerId(u64);

struct Entry<T> {
    due: i64,
    period_ms: i64,
    payload: T,
}

pub const MAX_FIRES_PER_INSTANT: u32 = 10_000;

/// Per-run counters of [`Scheduler::pop_due`]; one per `run_until`-style loop, so nested runs keep their own.
pub struct Run {
    fired: u32,
    instant: i64,
}

impl Default for Run {
    fn default() -> Self {
        Run {
            fired: 0,
            instant: -1,
        }
    }
}

pub enum Due<T> {
    /// A timer is due: the clock is at its due time. Fire it with no borrow held, then call [`Scheduler::rearm`].
    Fire {
        id: TimerId,
        payload: T,
        periodic: bool,
    },
    /// Too many timers fired at one instant; those due now were dropped.
    Overflow(String),
}

/// Timer queue as 0.1 `clock.Scheduler`, driven step by step so firing a timer never holds the queue.
pub struct Scheduler<T> {
    pub clock: VirtualClock,
    heap: BinaryHeap<Reverse<(i64, u64, u64)>>,
    timers: BTreeMap<u64, Entry<T>>,
    seq: u64,
}

impl<T: Clone> Scheduler<T> {
    pub fn new(clock: VirtualClock) -> Self {
        Scheduler {
            clock,
            heap: BinaryHeap::new(),
            timers: BTreeMap::new(),
            seq: 0,
        }
    }

    pub fn add(&mut self, delay_ms: i64, payload: T, period_ms: i64) -> TimerId {
        self.seq += 1;
        let id = self.seq;
        let due = self.clock.ms + delay_ms.max(0);
        self.timers.insert(
            id,
            Entry {
                due,
                period_ms,
                payload,
            },
        );
        self.push(id, due);
        TimerId(id)
    }

    fn push(&mut self, id: u64, due: i64) {
        self.seq += 1;
        self.heap.push(Reverse((due, self.seq, id)));
    }

    pub fn cancel(&mut self, id: TimerId) {
        self.timers.remove(&id.0);
    }

    pub fn next_due(&mut self) -> Option<i64> {
        while let Some(Reverse((due, _, id))) = self.heap.peek() {
            match self.timers.get(id) {
                Some(e) if e.due == *due => return Some(*due),
                _ => {
                    self.heap.pop();
                }
            }
        }
        None
    }

    /// The next timer due at or before `target_ms`, with the clock moved to its due time.
    pub fn pop_due(&mut self, target_ms: i64, run: &mut Run) -> Option<Due<T>> {
        let due = self.next_due().filter(|&due| due <= target_ms)?;
        let Some(Reverse((_, _, id))) = self.heap.pop() else {
            unreachable!()
        };
        if due > self.clock.ms {
            self.clock.advance(due - self.clock.ms);
        }
        if due != run.instant {
            (run.fired, run.instant) = (0, due);
        }
        run.fired += 1;
        if run.fired > MAX_FIRES_PER_INSTANT {
            // a timer re-arming itself with delay 0 would otherwise never let virtual time move on
            self.timers.retain(|&k, e| k != id && e.due > due);
            return Some(Due::Overflow(format!(
                "more than {MAX_FIRES_PER_INSTANT} timers fired at the same virtual instant \
                 (a callback re-arms itself with delay 0?); timers due now were dropped"
            )));
        }
        let e = &self.timers[&id];
        let (payload, periodic) = (e.payload.clone(), e.period_ms > 0);
        if !periodic {
            self.timers.remove(&id);
        }
        Some(Due::Fire {
            id: TimerId(id),
            payload,
            periodic,
        })
    }

    /// Re-queues a fired periodic timer unless it was cancelled meanwhile; a no-op for one-shot timers.
    pub fn rearm(&mut self, id: TimerId) {
        if let Some(e) = self.timers.get_mut(&id.0) {
            e.due += e.period_ms;
            let due = e.due;
            self.push(id.0, due);
        }
    }

    /// Ends a run: the clock moves on to `target_ms` if it is not there yet.
    pub fn advance_to(&mut self, target_ms: i64) {
        if target_ms > self.clock.ms {
            self.clock.advance(target_ms - self.clock.ms);
        }
    }

    pub fn clear(&mut self) {
        self.timers.clear();
        self.heap.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> VirtualClock {
        VirtualClock::new(local_ms(2026, 11, 20, 9, 0, 0, 0))
    }

    #[test]
    fn test_parse_duration() {
        let got: Vec<i64> = ["350", "350ms", "61s", "10m", "2h"]
            .iter()
            .map(|s| parse_duration(s).unwrap())
            .collect();
        assert_eq!(got, [350, 350, 61000, 600000, 7200000]);
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("soon").is_err());
    }

    #[test]
    fn test_scheduler_fires_in_order_and_period() {
        let mut s = Scheduler::new(clock());
        let mut seen = vec![];
        s.add(200, "b", 0);
        s.add(100, "a", 0);
        let t = s.add(50, "p", 50);
        let run = |s: &mut Scheduler<&'static str>, target: i64, seen: &mut Vec<(&str, i64)>| {
            let mut r = Run::default();
            while let Some(due) = s.pop_due(target, &mut r) {
                if let Due::Fire { id, payload, .. } = due {
                    seen.push((payload, s.clock.ms));
                    s.rearm(id);
                }
            }
            s.advance_to(target);
        };
        run(&mut s, 160, &mut seen);
        s.cancel(t);
        run(&mut s, 400, &mut seen);
        assert_eq!(
            seen,
            [("p", 50), ("a", 100), ("p", 100), ("p", 150), ("b", 200)]
        );
        assert_eq!(s.clock.now(), (2026, 11, 20, 9, 0, 0, 400));
    }

    #[test]
    fn test_next_due_skips_cancelled() {
        let mut s = Scheduler::new(clock());
        assert_eq!(s.next_due(), None);
        let a = s.add(10, (), 0);
        s.add(30, (), 0);
        s.cancel(a);
        assert_eq!(s.next_due(), Some(30));
    }

    #[test]
    fn test_periodic_timer_can_cancel_itself() {
        let mut s = Scheduler::new(clock());
        let mut n = vec![];
        s.add(10, (), 10);
        let mut r = Run::default();
        while let Some(due) = s.pop_due(100, &mut r) {
            if let Due::Fire { id, .. } = due {
                n.push(s.clock.ms);
                if n.len() == 2 {
                    s.cancel(id);
                }
                s.rearm(id);
            }
        }
        s.advance_to(100);
        assert_eq!(n, [10, 20]);
    }

    #[test]
    fn test_scheduler_zero_delay_loop_is_cut() {
        #[derive(Clone)]
        enum T {
            Again,
            Noop,
        }
        let mut s = Scheduler::new(clock());
        let (mut msgs, mut n) = (vec![], 0);
        s.add(0, T::Again, 0);
        s.add(0, T::Noop, 0);
        let mut r = Run::default();
        while let Some(due) = s.pop_due(1000, &mut r) {
            match due {
                Due::Fire { id, payload, .. } => {
                    if let T::Again = payload {
                        n += 1;
                        s.add(0, T::Again, 0);
                    }
                    s.rearm(id);
                }
                Due::Overflow(msg) => msgs.push(msg),
            }
        }
        s.advance_to(1000);
        assert!((9_990..=10_000).contains(&n), "{n}");
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("same virtual instant"), "{}", msgs[0]);
        assert_eq!(s.clock.ms, 1000);
        assert_eq!(s.next_due(), None);
    }
}
