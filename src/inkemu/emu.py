"""`Emu` over the Rust core (`inkemu._core.Session`), with the 0.1 public API."""

import json
from collections.abc import Callable
from datetime import datetime
from pathlib import Path
from typing import Any

from PIL import Image

from ._core import INSTRUCTION_BUDGET, STORAGE_QUOTA, Session, parse_duration

NET_MODES = ("replay", "record", "live")
KEYS = ("up", "down", "left", "right", "ok", "back")
SWIPES = ("left", "right", "up", "down")
HOLD_MS = 600


class Clock:
    """Virtual time: `ms` since start and the local wall-clock `now()`."""

    def __init__(self, session: Session):
        self._s = session

    @property
    def ms(self) -> int:
        return self._s.clock_ms()

    def now(self) -> datetime:
        y, mo, d, h, mi, s, ms = self._s.now_local()
        return datetime(y, mo, d, h, mi, s, ms * 1000)


def _image(frame: tuple[int, int, bytes]) -> Image.Image:
    w, h, data = frame
    return Image.frombytes("1", (w, h), data)


class Emu:
    """One emulated InkApp: Lua state, ctx, virtual time, screen and lifecycle."""

    instruction_budget = INSTRUCTION_BUDGET  # per outermost callback; tests lower it on the class
    storage_quota = STORAGE_QUOTA  # (bytes, files) of ctx.storage and again of ctx.files; tests lower it on the class

    def __init__(
        self,
        app_dir: str | Path,
        *,
        state_dir: str | Path | None = None,
        fixtures_dir: str | Path | None = None,
        net: str = "replay",
        start_time: str | datetime | None = None,
        firmware_api: tuple[int, int] = (2, 33),
        fresh: bool = False,
        secrets_file: str | Path | None = None,
        seed_files: bool | None = None,
        random_seed: int | str | None = None,
        fonts_dir: str | Path | None = None,
    ):
        self._s = Session(
            Path(app_dir),
            state_dir=Path(state_dir) if state_dir else None,
            fixtures_dir=Path(fixtures_dir) if fixtures_dir else None,
            net=net,
            start_time=start_time.isoformat() if isinstance(start_time, datetime) else start_time,
            firmware_api=tuple(firmware_api),
            fresh=fresh,
            secrets_file=Path(secrets_file) if secrets_file else None,
            seed_files=seed_files,
            random_seed=None if random_seed is None else str(random_seed),
            instruction_budget=self.instruction_budget,
            storage_quota=tuple(self.storage_quota),
            fonts_dir=Path(fonts_dir) if fonts_dir else None,
        )
        self.net_mode = net
        self.firmware_api = tuple(firmware_api)
        self.app_dir = Path(self._s.app_dir())
        self.state_dir = Path(self._s.state_dir())
        self.fixtures_dir = Path(self._s.fixtures_dir())
        self.secrets_file = Path(self._s.secrets_file())
        self.clock = Clock(self._s)
        self.on_pending: Callable[[dict], None] | None = None
        self._locale = "en"

    def _run(self, fn: Callable[..., Any], *args) -> Any:
        """Calls into the core; tells `on_pending` about a dialog that opened meanwhile."""
        before = self._s.pending_id()
        try:
            return fn(*args)
        finally:
            if self.on_pending and self._s.pending_id() != before and (p := self.pending) is not None:
                self.on_pending(p)

    # ---- input ------------------------------------------------------------------------------

    def pointer(self, phase: str, x: int, y: int) -> None:
        self._run(self._s.pointer, phase, x, y)

    def tap(self, x: int, y: int) -> None:
        self._run(self._s.tap, x, y)

    def double_tap(self, x: int, y: int) -> None:
        self._run(self._s.double_tap, x, y)

    def hold(self, x: int, y: int, ms: int = 800) -> None:
        self._run(self._s.hold, x, y, ms)

    def drag(self, points: list[tuple[int, int]], *, duration_ms: int = 300) -> None:
        self._run(self._s.drag, [tuple(p) for p in points], duration_ms)

    def swipe(self, direction: str, *, at: tuple[int, int] = (240, 400), distance: int = 200) -> None:
        self._run(self._s.swipe, direction, tuple(at), distance)

    def tap_text(self, text: str) -> None:
        self._run(self._s.tap_text, text)

    def key(self, name: str) -> None:
        self._run(self._s.key, name)

    def type_text(self, text: str) -> None:
        """Answer a pending `input:text` with OK, or type into a pending `input:edit` and press the confirm key.

        A newline is the confirm key too; with `enter = "newline"` it inserts a line break and the edit stays open.
        """
        self._run(self._s.type_text, str(text))

    def submit_text(self, text: str) -> None:
        """Submit a text field: answer a pending `input:text` with `text`, or replace a pending `input:edit`'s text
        with it and press the confirm key."""
        self._run(self._s.submit_text, str(text))

    def answer(self, ok: bool) -> None:
        """Answer the pending dialog: confirm yes/no, leave yes/no, text OK/cancel, edit confirm key/Back."""
        self._run(self._s.answer, bool(ok))

    # ---- time and lifecycle -----------------------------------------------------------------

    def wait(self, ms: int | str) -> None:
        """Jump the clock forward: timers fire and `clock:now().revision` changes."""
        self._run(self._s.wait, parse_duration(ms) if isinstance(ms, str) else int(ms))

    def advance(self, ms: int) -> None:
        """Let real time pass (the window's ticks): like `wait`, but `clock:now().revision` stays."""
        self._run(self._s.advance, int(ms))

    def leave(self) -> None:
        self._run(self._s.leave)

    def restart(self) -> None:
        self._run(self._s.restart)

    def reload(self) -> None:
        self._run(self._s.reload)

    def lock(self) -> None:
        self._run(self._s.lock)

    def unlock(self) -> None:
        self._run(self._s.unlock)

    def resume(self) -> None:
        self._run(self._s.resume)

    # ---- screen -----------------------------------------------------------------------------

    def shot(self, *, dialog: bool = True) -> Image.Image:
        """The screen with the error, toast and (unless `dialog=False`, for a window with its own) dialog plates."""
        return _image(self._s.frame(dialog))

    def raw_frame(self) -> Image.Image:
        """The app's own frame, without plates."""
        return _image(self._s.raw_frame())

    def save_shot(self, path: str | Path) -> Path:
        p = Path(path)
        p.parent.mkdir(parents=True, exist_ok=True)
        self.shot().save(p)
        return p

    def texts(self) -> list[dict]:
        return [
            {"text": text, "x": x, "y": y, "w": w, "h": h, "font": font} for text, x, y, w, h, font in self._s.texts()
        ]

    def has_text(self, s: str) -> bool:
        return any(s in t["text"] for t in self.texts())

    # ---- inspection -------------------------------------------------------------------------

    def status(self) -> dict:
        time, net, cmds, paused, screen, app_id, running, pending, locked, refresh, refreshes = self._s.status()
        return {
            "time": time,
            "net": net,
            "frame_commands": cmds,
            "paused": paused,
            "screen": list(screen),
            "app_id": app_id,
            "running": running,
            "pending": None if pending is None else dict(zip(("kind", "title", "text"), pending, strict=True)),
            "locked": locked,
            "refresh": refresh,
            "refreshes": refreshes,
        }

    @property
    def log(self) -> list[str]:
        return self._s.log()

    @property
    def errors(self) -> list[str]:
        return self._s.errors()

    @property
    def warnings(self) -> list[str]:
        return self._s.warnings()

    @property
    def toasts(self) -> list[str]:
        return self._s.toasts()

    @property
    def pending(self) -> dict | None:
        return self.status()["pending"]

    @property
    def pending_id(self) -> int:
        """Changes whenever a new dialog becomes pending."""
        return self._s.pending_id()

    @property
    def running(self) -> bool:
        return self.status()["running"]

    @property
    def locked(self) -> bool:
        return self.status()["locked"]

    @property
    def app_id(self) -> str:
        return self.status()["app_id"]

    @property
    def frame_commands(self) -> int:
        return self.status()["frame_commands"]

    @property
    def paused(self) -> bool:
        return self.status()["paused"]

    @paused.setter
    def paused(self, value: bool) -> None:
        self._s.set_paused(bool(value))

    @property
    def locale(self) -> str:
        return self._locale

    @locale.setter
    def locale(self, value: str) -> None:
        self._locale = value
        self._s.set_locale(value)

    @property
    def lock_wakes(self) -> int:
        return self._s.lock_wakes()

    def log_line(self, msg: str) -> None:
        self._s.log_line(msg)

    # ---- test helpers -----------------------------------------------------------------------

    def lua_exec(self, code: str) -> None:
        """Run Lua code in the app's state under the instruction budget."""
        self._run(self._s.lua_exec, code)

    def lua_eval(self, code: str) -> Any:
        """Run Lua code in the app's state under the instruction budget; its first result as plain Python data."""
        return json.loads(self._run(self._s.lua_eval, code))
