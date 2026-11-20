import heapq
import itertools
import re
from datetime import datetime, timedelta
from typing import Callable

_DURATION = re.compile(r"^\s*(\d+(?:\.\d+)?)\s*(ms|s|m|h)?\s*$")
_UNIT_MS = {None: 1, "ms": 1, "s": 1000, "m": 60_000, "h": 3_600_000}


def parse_duration(s: str) -> int:
    m = _DURATION.match(s)
    if not m:
        raise ValueError(f"bad duration {s!r}: use 350, 350ms, 61s, 10m or 2h")
    return round(float(m.group(1)) * _UNIT_MS[m.group(2)])


class VirtualClock:
    def __init__(self, start: datetime):
        self._start = start
        self.ms = 0

    def now(self) -> datetime:
        return self._start + timedelta(milliseconds=self.ms)

    def advance(self, ms: int) -> None:
        self.ms += ms


class Timer:
    def __init__(self, due: int, fn: Callable[[], None], period_ms: int):
        self.due, self.fn, self.period_ms, self.cancelled = due, fn, period_ms, False

    def cancel(self) -> None:
        self.cancelled = True


class Scheduler:
    def __init__(self, clock: VirtualClock):
        self.clock = clock
        self._heap: list[tuple[int, int, Timer]] = []
        self._seq = itertools.count()

    def add(self, delay_ms: int, fn: Callable[[], None], *, period_ms: int = 0) -> Timer:
        t = Timer(self.clock.ms + max(0, delay_ms), fn, period_ms)
        self._push(t)
        return t

    def _push(self, t: Timer) -> None:
        heapq.heappush(self._heap, (t.due, next(self._seq), t))

    def next_due(self) -> int | None:
        while self._heap and self._heap[0][2].cancelled:
            heapq.heappop(self._heap)
        return self._heap[0][0] if self._heap else None

    def run_until(self, target_ms: int) -> None:
        while (due := self.next_due()) is not None and due <= target_ms:
            _, _, t = heapq.heappop(self._heap)
            if due > self.clock.ms:
                self.clock.advance(due - self.clock.ms)
            t.fn()
            if t.period_ms > 0 and not t.cancelled:
                t.due = due + t.period_ms
                self._push(t)
        if target_ms > self.clock.ms:
            self.clock.advance(target_ms - self.clock.ms)
