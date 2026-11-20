from datetime import datetime

import pytest

from inkemu.clock import Scheduler, VirtualClock, parse_duration


def test_parse_duration():
    assert [parse_duration(s) for s in ("350", "350ms", "61s", "10m", "2h")] == [350, 350, 61000, 600000, 7200000]


def test_parse_duration_invalid():
    with pytest.raises(ValueError):
        parse_duration("soon")


def test_scheduler_fires_in_order_and_period():
    c = VirtualClock(datetime(2026, 11, 20, 9))
    s = Scheduler(c)
    seen = []
    s.add(200, lambda: seen.append(('b', c.ms)))
    s.add(100, lambda: seen.append(('a', c.ms)))
    t = s.add(50, lambda: seen.append(('p', c.ms)), period_ms=50)
    s.run_until(160)
    t.cancel()
    s.run_until(400)
    assert seen == [('p', 50), ('a', 100), ('p', 100), ('p', 150), ('b', 200)]
    assert c.now() == datetime(2026, 11, 20, 9, 0, 0, 400000)


def test_next_due_skips_cancelled():
    c = VirtualClock(datetime(2026, 11, 20, 9))
    s = Scheduler(c)
    assert s.next_due() is None
    a = s.add(10, lambda: None)
    s.add(30, lambda: None)
    a.cancel()
    assert s.next_due() == 30


def test_periodic_timer_can_cancel_itself():
    c = VirtualClock(datetime(2026, 11, 20, 9))
    s = Scheduler(c)
    n = []
    holder = {}
    def tick():
        n.append(c.ms)
        if len(n) == 2:
            holder["t"].cancel()
    holder["t"] = s.add(10, tick, period_ms=10)
    s.run_until(100)
    assert n == [10, 20]
