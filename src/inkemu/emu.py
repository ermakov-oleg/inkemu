import json
import os
import random
import re
import shutil
from datetime import datetime
from pathlib import Path
from typing import Any, Callable

from lupa import lua55
from PIL import Image, ImageDraw

from . import ctx_basic, ctx_ui, lockscreen, net as net_mod
from .clock import Scheduler, Timer, VirtualClock, parse_duration
from .draw import Canvas
from .fonts import Fonts
from .lua_env import load_chunk, make_object, new_runtime, protected_call

NET_MODES = ("replay", "record", "live")
PERMISSIONS = ("net", "reader", "ble")
TOAST_MS = 2000
KEYS = ("up", "down", "left", "right", "ok", "back")
INPUT_MS = 350
HOLD_MS = 600
POINTER_PHASES = ("down", "move", "hold", "up")
SWIPES = {"left": (-1, 0), "right": (1, 0), "up": (0, -1), "down": (0, 1)}
ERROR_TITLE = "App error, closed"
LOCK_WAIT_MS = 30_000

_REQUIRE = r"""
local find, load_chunk, error = ...
package.searchers = { function(name)
  local path, src = find(name)
  if not path then return src end
  local f, err = load_chunk(src, '@' .. path)
  if not f then error("error loading module '" .. name .. "' from file '" .. path .. "':\n\t" .. err, 0) end
  return f, path
end }
"""
_MODULE_NAME = re.compile(r"[A-Za-z0-9_-]+(\.[A-Za-z0-9_-]+)*")


def _parse_start(start_time: str | datetime | None) -> datetime:
    if start_time is None:
        return datetime.now().replace(microsecond=0)
    dt = start_time if isinstance(start_time, datetime) else datetime.fromisoformat(start_time)
    return dt.astimezone().replace(tzinfo=None) if dt.tzinfo else dt


def _classify(dx: int, dy: int, held_ms: int) -> str | None:
    """Gesture on pointer up; a 20..60 px move that is not a hold has no gesture."""
    if max(abs(dx), abs(dy)) >= 60:
        if abs(dx) >= abs(dy):
            return "swipe_right" if dx > 0 else "swipe_left"
        return "swipe_down" if dy > 0 else "swipe_up"
    if held_ms >= HOLD_MS:
        return "long"
    return "tap" if max(abs(dx), abs(dy)) < 20 else None


class Emu:
    """One emulated InkApp: Lua state, ctx, virtual time, screen and lifecycle."""

    def __init__(self, app_dir: str | Path, *, state_dir: str | Path | None = None,
                 fixtures_dir: str | Path | None = None, net: str = "replay",
                 start_time: str | datetime | None = None, firmware_api: tuple[int, int] = (2, 26),
                 fresh: bool = False, secrets_file: str | Path | None = None, seed_files: bool | None = None,
                 random_seed: int | str | None = None):
        if net not in NET_MODES:
            raise ValueError(f"net must be one of {', '.join(NET_MODES)}, not {net!r}")
        self.app_dir = Path(app_dir).resolve()
        self.net_mode = net
        self.firmware_api = tuple(firmware_api)
        self.log: list[str] = []
        self.errors: list[str] = []
        self.warnings: list[str] = []
        self.paused = False
        self.pending: dict | None = None
        self.on_pending: Callable[[dict], None] | None = None
        self.toasts: list[str] = []
        self._pending_h: dict | None = None
        self._toast: tuple[str, int] | None = None
        self._images: dict[int, Any] = {}
        self.running = False
        self.locked = False
        self.lock_wakes = 0
        self.net_inflight = 0
        self._lock_ctx: dict | None = None
        self._lock_interval = 0
        self._lock_cycle_ms = 0
        self._lock_waiting = False
        self._lock_wake_timer: Timer | None = None
        self._lock_deadline: Timer | None = None
        self._canvas_stale = False
        self.battery = {"percent": 87, "charging": False, "voltage_mv": 4020, "temperature_c": 24}
        self.full_refresh_deferred = 0
        self.locale = "en"
        # uuids must not repeat across restarts; a seed makes them reproducible on purpose
        self.rng = random.SystemRandom() if random_seed is None else random.Random(random_seed)
        self.frontlight = 0
        self.refresh_counts = {"request": 0, "full": 0}
        self.frame_commands = 0
        self.clock = VirtualClock(_parse_start(start_time))
        self.scheduler = Scheduler(self.clock, on_overflow=self._fail)
        self.fonts = Fonts()
        self.L = None
        self.canvas: Canvas | None = None
        self._ctx = None
        self._install: list[Callable[["Emu"], None]] = [ctx_basic.install, net_mod.install, ctx_ui.install,
                                                  lockscreen.install]
        self._timers: set[Timer] = set()
        self._tick: Timer | None = None
        self._invalid = self._quit = self._leaving = False
        self._depth = 0
        self._overlay: tuple[str, str] | None = None
        self._down: tuple[int, int, int] | None = None
        self._hold_sent = False

        self.manifest, problem = self._read_manifest()
        app_id = self.manifest.get("app_id")
        # app_id comes from an untrusted manifest and names directories we may wipe
        if not (isinstance(app_id, str) and re.fullmatch(r"[A-Za-z]+", app_id)):
            app_id = re.sub(r"[^A-Za-z]", "", self.app_dir.name) or "app"
        self.app_id = app_id
        root = Path(os.environ.get("INKEMU_STATE_ROOT", ".emu")).absolute()
        self.state_dir = Path(state_dir) if state_dir else root / self.app_id
        self.fixtures_dir = Path(fixtures_dir) if fixtures_dir else Path("tests/fixtures").absolute() / self.app_id
        self.secrets_file = Path(secrets_file) if secrets_file else root / "secrets.json"
        if not state_dir and not self.state_dir.resolve().is_relative_to(root.resolve()):
            raise ValueError(f"state dir {self.state_dir} is outside the state root {root}")
        if fresh and not problem:
            self._wipe_state()
        if (fresh if seed_files is None else seed_files) and not problem:
            ctx_basic.seed_files(self)
        self._boot()

    # ---- public API -------------------------------------------------------------------------

    def pointer(self, phase: str, x: int, y: int) -> None:
        """Low-level pointer event; the caller owns the timing. `up` classifies the gesture and advances 350 ms."""
        if phase not in POINTER_PHASES:
            raise ValueError(f"unknown pointer phase {phase!r}: use one of {', '.join(POINTER_PHASES)}")
        if phase == "down":
            self._down = (x, y, self.clock.ms)
            self._hold_sent = False
        elif self._down is None:
            self._log_line(f"pointer {phase} without down ignored")
            return
        if phase == "hold":
            self._hold_sent = True
        dx, dy, t0 = self._down
        event = {"type": "pointer", "phase": phase, "x": x, "y": y, "down_x": dx, "down_y": dy,
                 "time_ms": self.clock.ms}
        if phase != "up":
            self._send(event)
            return
        self._down = None
        # a frozen window clock must not turn a delivered hold into a tap
        held = max(self.clock.ms - t0, HOLD_MS if self._hold_sent else 0)
        gesture = _classify(x - dx, y - dy, held)
        if gesture:
            event["gesture"] = gesture
        self._dispatch(event)

    def tap(self, x: int, y: int) -> None:
        self.pointer("down", x, y)
        self.pointer("up", x, y)

    def double_tap(self, x: int, y: int) -> None:
        self.tap(x, y)
        self.wait(100)
        self.pointer("down", x, y)
        self._down = None
        self._dispatch({"type": "pointer", "phase": "up", "gesture": "double_tap", "x": x, "y": y,
                        "down_x": x, "down_y": y, "time_ms": self.clock.ms})

    def hold(self, x: int, y: int, ms: int = 800) -> None:
        self.pointer("down", x, y)
        if ms >= HOLD_MS:
            self.wait(HOLD_MS)
            self.pointer("hold", x, y)
            ms -= HOLD_MS
        self.wait(ms)
        self.pointer("up", x, y)

    def drag(self, points: list[tuple[int, int]], *, duration_ms: int = 300) -> None:
        if len(points) < 2:
            raise ValueError("drag() needs at least two points")
        self.pointer("down", *points[0])
        step = max(duration_ms, 0) // (len(points) - 1)
        for p in points[1:]:
            self.wait(step)
            self.pointer("move", *p)
        self.pointer("up", *points[-1])

    def swipe(self, direction: str, *, at: tuple[int, int] = (240, 400), distance: int = 200) -> None:
        if direction not in SWIPES:
            raise ValueError(f"unknown swipe direction {direction!r}: use one of {', '.join(SWIPES)}")
        ux, uy = SWIPES[direction]
        steps = 5
        self.drag([(at[0] + ux * distance * i // steps, at[1] + uy * distance * i // steps)
                   for i in range(steps + 1)])

    def tap_text(self, text: str) -> None:
        items = self.texts()
        for match in (lambda t: t == text, lambda t: text in t):
            hits = [it for it in items if match(it["text"])]
            if hits:
                it = hits[-1]
                self.tap(it["x"] + it["w"] // 2, it["y"] + it["h"] // 2)
                return
        raise LookupError(f"no text {text!r} on screen; visible: {[it['text'] for it in items]}")

    def key(self, name: str) -> None:
        if name not in KEYS:
            raise ValueError(f"unknown key {name!r}: use one of {', '.join(KEYS)}")
        self._send({"type": "key", "key": name, "phase": "down", "time_ms": self.clock.ms})
        self._dispatch({"type": "key", "key": name, "phase": "up", "time_ms": self.clock.ms})

    def wait(self, ms: int | str) -> None:
        ms = parse_duration(ms) if isinstance(ms, str) else int(ms)
        if ms < 0:
            raise ValueError("wait() needs a non-negative duration")
        self.scheduler.run_until(self.clock.ms + ms)
        self._settle()

    def leave(self) -> None:
        if self.locked:
            self._end_lock()
            self._log_line("left while locked: the lock screen was dropped")
            return
        if not self.running or self._leaving:
            return
        self._leaving = True
        try:
            self._call("on_leave", self._ctx)
            if self.running:
                self.save_state()
        finally:
            self._leaving = self._quit = False
            self._stop()
        self._log_line("app left")

    def type_text(self, text: str) -> None:
        """Answer a pending `input:text` with OK, or type into a pending `input:edit` and press enter."""
        h = self._pending_h
        if self.pending is None or not h or not h.get("type"):
            raise RuntimeError(f"no text input is waiting; pending: {self.pending}")
        h["type"](str(text))
        self._settle()
        self.wait(INPUT_MS)

    def answer(self, ok: bool) -> None:
        """Answer the pending dialog: confirm yes/no, leave yes/no, text OK/cancel."""
        h = self._pending_h
        if self.pending is None or not h:
            raise RuntimeError("no dialog is waiting for an answer")
        self._clear_pending()
        h["answer"](bool(ok))
        self._settle()
        self.wait(INPUT_MS)

    def restart(self) -> None:
        self.leave()
        self._boot()

    def reload(self) -> None:
        if self.locked:
            self._end_lock()
        if self.running:
            self.save_state()
        self._boot()

    def resume(self) -> None:
        self.paused = False

    def save_state(self) -> None:
        ctx_basic.save_state(self)

    def invalidate(self) -> None:
        self._invalid = True

    def shot(self) -> Image.Image:
        try:
            img = self.canvas.render()
        except Exception as e:  # bad option values from the app (e.g. width='abc') surface only here
            self._fail(f"g: cannot draw the frame: {type(e).__name__}: {e}")
            self.canvas.begin_frame()
            img = self.canvas.render()
        d = ImageDraw.Draw(img)
        for items in self._plates():
            top, bottom = items[0]["y"] - 16, items[-1]["y"] + items[-1]["h"] + 16
            d.rectangle([16, top, self.canvas.width - 17, bottom], fill=255, outline=0, width=3)
            for it in items:
                f = self.fonts.pil(it["font"])
                d.text((it["x"], it["y"] + (it["h"] - f.size) / 2), it["text"], font=f, fill=0)
        return img

    def save_shot(self, path: str | Path) -> Path:
        p = Path(path)
        p.parent.mkdir(parents=True, exist_ok=True)
        self.shot().save(p)
        return p

    def texts(self) -> list[dict]:
        return self.canvas.texts() + self._overlay_items()

    def has_text(self, s: str) -> bool:
        return any(s in t["text"] for t in self.texts())

    def status(self) -> dict:
        return {
            "time": self.clock.now().isoformat(timespec="milliseconds"),
            "net": self.net_mode,
            "frame_commands": self.frame_commands,
            "paused": self.paused,
            "screen": [self.canvas.width, self.canvas.height],
            "app_id": self.app_id,
            "running": self.running,
            "pending": self.pending,
            "locked": self.locked,
        }

    # ---- lifecycle --------------------------------------------------------------------------

    def _boot(self, *, resume: bool = False, lock: bool = False) -> None:
        """Fresh Lua state for the app: manifest, ctx, entry chunk, on_load (+ on_resume), first frame."""
        self._stop()
        self._overlay, self._ctx, self._toast = None, None, None
        self._images = {}
        self._quit = False
        self.net_inflight = 0
        self._lock_waiting = False
        if not lock:
            self._lock_ctx = None
        self.manifest, problem = self._read_manifest()
        landscape = (self.manifest.get("display") or {}).get("orientation") == "landscape"
        self.L = new_runtime()
        # a timer wake keeps showing the previous frame until the new state draws (e-ink keeps its image)
        self._canvas_stale = bool(lock and self._lock_ctx["reason"] == "timer" and self.canvas is not None)
        if not self._canvas_stale:
            self.canvas = self._new_canvas(*((800, 480) if landscape else (480, 800)))
        self.frame_commands = 0
        if problem:
            self._fail(problem, "Install failed")
            return
        if lock:
            block = self._lock_block()
            if block is None:
                return
            entry = block.get("entry") or "lockscreen.lua"
        else:
            entry = self.manifest.get("entry", "index.lua")
        found, src = self._find_file(entry)
        if not found:
            self._fail(f"manifest.json: entry {entry!r} not found in the app folder", "Install failed")
            return
        self.L.execute(_REQUIRE, self._find_module, self.L._load, self.L.globals().error)
        self._ctx = make_object(self.L, "ctx", {})
        for hook in self._install:
            hook(self)
        self.L.globals().ctx = self._ctx
        fn, err = load_chunk(self.L, src, "@" + found)
        if fn is None:
            self._fail(f"{found}: {err}")
            return
        self.running = True
        self._invalid = True
        self._log_line(f"app {self.app_id} started")
        self._protected(fn, where=found)
        self._call("on_load", self._ctx)
        if resume:
            self._call("on_resume", self._ctx)
        if lock:
            self._lock_after_load()
        else:
            self._settle()

    def _new_canvas(self, w: int, h: int) -> Canvas:
        canvas = Canvas(self.L, self.fonts, w, h)
        canvas.on_warning = self._warn
        return canvas

    # ---- lock screen ------------------------------------------------------------------------

    def lock(self) -> None:
        """Leave the app, then run the manifest's lock screen (a blank screen without one)."""
        if self.locked:
            return
        self.leave()
        self.locked = True
        self.lock_wakes = 0
        self._lock_interval = int(self._lock_block_get("interval_sec", 0))
        self._lock_enter("lock")

    def unlock(self) -> None:
        """Drop the lock screen state and start the app again (on_load + on_resume, saved state kept)."""
        if not self.locked:
            return
        self._end_lock()
        self._boot(resume=True)

    def _lock_block(self) -> dict | None:
        block = self.manifest.get("lockscreen")
        return block if isinstance(block, dict) and block.get("enabled", True) else None

    def _lock_block_get(self, key: str, default):
        v = (self._lock_block() or {}).get(key, default)
        return v if isinstance(v, (int, float)) and not isinstance(v, bool) and v > 0 else default

    def _net_enabled(self) -> bool:
        if self._lock_ctx is not None:
            return self._lock_ctx["net"]
        return "net" in (self.manifest.get("permissions") or [])

    def _lock_enter(self, reason: str) -> None:
        block = self._lock_block() or {}
        now = self.clock.ms
        self._lock_ctx = {"reason": reason, "net": bool(block.get("net")), "done": False,
                          "elapsed": (now - self._lock_cycle_ms) // 1000 if reason == "timer" else 0}
        self._lock_cycle_ms = now
        self._boot(lock=True)
        self._lock_schedule()

    def _lock_schedule(self) -> None:
        if self._lock_wake_timer:
            self._cancel_timer(self._lock_wake_timer)
            self._lock_wake_timer = None
        if self.locked and self.running and self._lock_interval > 0:
            due = self._lock_cycle_ms + int(self._lock_interval * 1000) - self.clock.ms
            self._lock_wake_timer = self._add_timer(due, self._lock_wake)

    def _lock_wake(self) -> None:
        self.lock_wakes += 1
        self._log_line("lock screen wake")
        self._lock_enter("timer")

    def _set_lock_interval(self, sec: float) -> None:
        self._lock_interval = sec
        self._lock_schedule()

    def _lock_ready(self) -> bool:
        return self._lock_ctx["done"] or self.net_inflight == 0

    def _lock_after_load(self) -> None:
        if not self.running:
            return
        if self._lock_ctx["reason"] == "lock" or self._lock_ready():
            self._lock_present()
        else:
            self._lock_waiting = True
            self._lock_deadline = self._add_timer(LOCK_WAIT_MS, self._lock_present)

    def _lock_present(self) -> None:
        if self._lock_waiting:
            self._lock_waiting = False
            self._cancel_timer(self._lock_deadline)
        if self._canvas_stale:
            self._canvas_stale = False
            self.canvas = self._new_canvas(self.canvas.width, self.canvas.height)
        self._invalid = True
        self._settle()

    def _end_lock(self) -> None:
        self._stop()
        self.locked = False
        self._lock_ctx = None
        self._lock_waiting = self._canvas_stale = False
        self._lock_wake_timer = None

    def _stop(self) -> None:
        self.running = False
        self._clear_pending()
        self._invalid = False
        for t in self._timers:
            t.cancel()
        self._timers.clear()
        self._tick = None

    def _fail(self, message: str, title: str = ERROR_TITLE) -> None:
        self.errors.append(message)
        self._log_line("ERROR " + message)
        self._overlay = (title, message.split("\n", 1)[0])
        self._stop()

    def _read_manifest(self) -> tuple[dict, str | None]:
        try:
            m = json.loads((self.app_dir / "manifest.json").read_text(encoding="utf-8"))
        except FileNotFoundError:
            return {}, f"manifest.json not found in {self.app_dir}"
        except (OSError, ValueError) as e:
            return {}, f"manifest.json is not valid JSON: {e}"
        if not isinstance(m, dict):
            return {}, "manifest.json must be a JSON object"
        problems = []
        app_id = m.get("app_id")
        if not isinstance(app_id, str) or not re.fullmatch(r"[A-Za-z]+", app_id):
            problems.append(f"app_id {app_id!r} must be Latin letters only")
        perms = m.get("permissions", [])
        bad = [p for p in perms if p not in PERMISSIONS] if isinstance(perms, list) else [perms]
        if bad:
            problems.append(f"unknown permission(s) {', '.join(map(str, bad))}; allowed: {', '.join(PERMISSIONS)}")
        api = m.get("api", "0.8")
        api_m = re.fullmatch(r"\s*(\d+)\.(\d+)\s*", str(api))
        fw = ".".join(map(str, self.firmware_api))
        if not api_m:
            problems.append(f"api {api!r} is not 'major.minor'")
        elif (int(api_m[1]), int(api_m[2])) > self.firmware_api:
            problems.append(f"app needs firmware with API {api_m[1]}.{api_m[2]}; the emulated firmware has {fw}")
        return m, ("manifest.json: " + "; ".join(problems)) if problems else None

    def _find_file(self, rel: str) -> tuple[str | None, bytes | None]:
        try:
            p = (self.app_dir / rel).resolve()
            if not p.is_relative_to(self.app_dir) or not p.is_file():
                return None, None
            return p.relative_to(self.app_dir).as_posix(), p.read_bytes()
        except (OSError, ValueError):
            return None, None

    def _find_module(self, name):
        rel = str(name).replace(".", "/") + ".lua"
        if isinstance(name, str) and _MODULE_NAME.fullmatch(name):
            found, src = self._find_file(rel)
            if found:
                return found, src
        return None, f"\n\tno file '{rel}' in the app folder"

    def _wipe_state(self) -> None:
        for name in ("state.json", "lock.json"):
            (self.state_dir / name).unlink(missing_ok=True)
        for name in ("files", "shots"):
            shutil.rmtree(self.state_dir / name, ignore_errors=True)

    # ---- calls into Lua ---------------------------------------------------------------------

    def _protected(self, fn, *args, where: str) -> tuple[bool, tuple]:
        """pcall-equivalent: a Lua error lands in `errors`, shows the error overlay and stops the app."""
        if not self.running:
            return False, ()
        self._depth += 1
        try:
            ok, res = protected_call(self.L, fn, *args)
        finally:
            self._depth -= 1
        if not ok:
            self._fail(f"Lua error in {where}: {res[0] if res else 'unknown error'}")
            return False, ()
        if self._quit and self._depth == 0 and not self._leaving:
            self.leave()
        return True, res

    def _call(self, name: str, *args) -> Any:
        """Call the app's global callback `name` if it is defined; returns its first result."""
        if not self.running:
            return None
        fn = self.L.globals()[name]
        if lua55.lua_type(fn) != "function":
            return None
        ok, res = self._protected(fn, *args, where=name)
        return res[0] if ok and res else None

    def _request_quit(self) -> None:
        if self.locked:
            self._log_line("ctx:quit ignored on the lock screen")
            return
        self._quit = True

    def _add_timer(self, delay_ms: int, fn: Callable[[], Any], *, period_ms: int = 0) -> Timer:
        def fire():
            if not timer.period_ms:
                self._timers.discard(timer)
            fn()
            self._settle()

        timer = self.scheduler.add(delay_ms, fire, period_ms=period_ms)
        self._timers.add(timer)
        return timer

    def _cancel_timer(self, timer: Timer) -> None:
        timer.cancel()
        self._timers.discard(timer)

    def _set_tick(self, ms: int) -> None:
        if self._tick:
            self._cancel_timer(self._tick)
            self._tick = None
        if ms > 0:
            self._tick = self._add_timer(ms, lambda: self._call("on_tick", self._ctx), period_ms=ms)

    # ---- events and drawing -----------------------------------------------------------------

    def _send(self, event: dict) -> None:
        """Deliver one input event to on_input and redraw if needed, without advancing time."""
        if self.locked:
            self._log_line(f"input ignored, the screen is locked: {event['type']} {event['phase']}")
            return
        if not self.running:
            self._log_line(f"input ignored, the app is not running: {event['type']} {event['phase']}")
            return
        handled = self._call("on_input", self._ctx, self.L.table_from(event))
        if (event["type"] == "key" and event["key"] == "back" and self.running and not handled
                and self.pending is None):
            self._set_pending("leave", "Leave app?", "", answer=lambda ok: self.leave() if ok else None)
            self._log_line("back not handled by on_input: the host offers to leave")
        self._settle()

    def _dispatch(self, event: dict) -> None:
        self._send(event)
        self.wait(INPUT_MS)

    def _settle(self) -> None:
        if self._lock_waiting:
            if self._lock_ready():
                self._lock_present()
            return
        if self._invalid and self.running:
            self._invalid = False
            self._draw()

    def _draw(self) -> None:
        self.canvas.begin_frame()
        self._call("on_draw", self._ctx, self.canvas.g)
        try:
            self.frame_commands = self.canvas.end_frame()
        except Exception as e:
            self._fail(f"g: bad values in on_draw: {type(e).__name__}: {e}")
            self.canvas.begin_frame()

    def _set_pending(self, kind: str, title: str, text: str, *, answer: Callable[[bool], Any],
                     type_text: Callable[[str], Any] | None = None) -> dict:
        if self.pending is not None:
            self._warn(f"a {self.pending['kind']} dialog was still open; replaced by {kind}")
        self.pending = {"kind": kind, "title": title, "text": text}
        self._pending_h = {"answer": answer, "type": type_text}
        self._log_line(f"pending {kind}: {title}")
        if self.on_pending:
            self.on_pending(dict(self.pending))
        return self._pending_h

    def _clear_pending(self) -> None:
        self.pending = self._pending_h = None

    def _show_toast(self, text: str) -> None:
        self.toasts.append(text)
        self._toast = (text, self.clock.ms + TOAST_MS)
        self._log_line(f"toast: {text}")

    def _plates(self) -> list[list[dict]]:
        plates = []
        if self._overlay:
            title, msg = self._overlay
            w = self.canvas.width
            items, y = [], self.canvas.height // 2 - 60
            for text, font in [(title, "large")] + [(line, "small") for line in self.canvas._wrap("small", msg, w - 64)[:4]]:
                tw, lh = self.fonts.measure(font, text), self.fonts.line_height(font)
                items.append({"text": text, "x": round((w - tw) / 2), "y": y, "w": tw, "h": lh, "font": font})
                y += lh + (8 if font == "large" else 0)
            plates.append(items)
        if self._toast and self.clock.ms < self._toast[1]:
            w, lh = self.canvas.width, self.fonts.line_height("medium")
            lines = self.canvas._wrap("medium", self._toast[0], w - 96)[:3]
            y = self.canvas.height - 48 - 16 - lh * len(lines)
            items = []
            for line in lines:
                tw = self.fonts.measure("medium", line)
                items.append({"text": line, "x": round((w - tw) / 2), "y": y, "w": tw, "h": lh, "font": "medium"})
                y += lh
            plates.append(items)
        return plates

    def _overlay_items(self) -> list[dict]:
        return [it for plate in self._plates() for it in plate]

    # ---- log --------------------------------------------------------------------------------

    def _log_line(self, msg: str) -> None:
        t = self.clock.now()
        self.log.append(f"{t:%H:%M:%S}.{t.microsecond // 1000:03d} {msg}")

    def _warn(self, msg: str) -> None:
        self.warnings.append(msg)
        self._log_line("WARN " + msg)
