import json
import os
import re
import shutil
from datetime import datetime
from pathlib import Path
from typing import Any, Callable

from lupa import lua55
from PIL import Image, ImageDraw

from . import ctx_basic
from .clock import Scheduler, Timer, VirtualClock, parse_duration
from .draw import Canvas
from .fonts import Fonts
from .lua_env import load_chunk, make_object, new_runtime, protected_call

NET_MODES = ("replay", "record", "live")
PERMISSIONS = ("net", "reader", "ble")
KEYS = ("up", "down", "left", "right", "ok", "back")
INPUT_MS = 350
ERROR_TITLE = "App error, closed"

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


class Emu:
    """One emulated InkApp: Lua state, ctx, virtual time, screen and lifecycle."""

    def __init__(self, app_dir: str | Path, *, state_dir: str | Path | None = None,
                 fixtures_dir: str | Path | None = None, net: str = "replay",
                 start_time: str | datetime | None = None, firmware_api: tuple[int, int] = (2, 26),
                 fresh: bool = False, secrets_file: str | Path | None = None):
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
        self.running = False
        self.battery = {"percent": 87, "charging": False, "voltage_mv": 4020, "temperature_c": 24}
        self.full_refresh_deferred = 0
        self.frame_commands = 0
        self.clock = VirtualClock(_parse_start(start_time))
        self.scheduler = Scheduler(self.clock)
        self.fonts = Fonts()
        self.L = None
        self.canvas: Canvas | None = None
        self._ctx = None
        self._install: list[Callable[["Emu"], None]] = [ctx_basic.install]
        self._timers: set[Timer] = set()
        self._tick: Timer | None = None
        self._invalid = self._quit = self._leaving = False
        self._depth = 0
        self._overlay: tuple[str, str] | None = None

        self.manifest, _ = self._read_manifest()
        app_id = self.manifest.get("app_id")
        self.app_id = app_id if isinstance(app_id, str) and app_id else self.app_dir.name
        root = Path(os.environ.get("INKEMU_STATE_ROOT", ".emu")).absolute()
        self.state_dir = Path(state_dir) if state_dir else root / self.app_id
        self.fixtures_dir = Path(fixtures_dir) if fixtures_dir else Path("tests/fixtures").absolute() / self.app_id
        self.secrets_file = Path(secrets_file) if secrets_file else root / "secrets.json"
        if fresh:
            self._wipe_state()
        self._boot()

    # ---- public API -------------------------------------------------------------------------

    def tap(self, x: int, y: int) -> None:
        base = {"type": "pointer", "x": x, "y": y, "down_x": x, "down_y": y, "time_ms": self.clock.ms}
        self._send({**base, "phase": "down"})
        self._dispatch({**base, "phase": "up", "gesture": "tap"})

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
        if not self.running or self._leaving:
            return
        self._leaving = True
        try:
            self._call("on_leave", self._ctx)
            if self.running:
                self.save_state()
        finally:
            self._leaving = self._quit = False
            self.pending = None
            self._stop()
        self._log_line("app left")

    def restart(self) -> None:
        self.leave()
        self._boot()

    def reload(self) -> None:
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
        img = self.canvas.render()
        items = self._overlay_items()
        if items:
            d = ImageDraw.Draw(img)
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
        }

    # ---- lifecycle --------------------------------------------------------------------------

    def _boot(self, *, resume: bool = False) -> None:
        """Fresh Lua state for the app: manifest, ctx, entry chunk, on_load (+ on_resume), first frame."""
        self._stop()
        self.pending, self._overlay, self._ctx = None, None, None
        self._quit = False
        self.manifest, problem = self._read_manifest()
        landscape = (self.manifest.get("display") or {}).get("orientation") == "landscape"
        self.L = new_runtime()
        self.canvas = Canvas(self.L, self.fonts, *((800, 480) if landscape else (480, 800)))
        self.canvas.on_warning = self._warn
        self.frame_commands = 0
        if problem:
            self._fail(problem, "Install failed")
            return
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
        self._settle()

    def _stop(self) -> None:
        self.running = False
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
        if not self.running:
            self._log_line(f"input ignored, the app is not running: {event['type']} {event['phase']}")
            return
        handled = self._call("on_input", self._ctx, self.L.table_from(event))
        if (event["type"] == "key" and event["key"] == "back" and self.running and not handled
                and self.pending is None):
            self.pending = {"kind": "leave", "title": "Leave app?", "text": ""}
            self._log_line("back not handled by on_input: the host offers to leave")
        self._settle()

    def _dispatch(self, event: dict) -> None:
        self._send(event)
        self.wait(INPUT_MS)

    def _settle(self) -> None:
        if self._invalid and self.running:
            self._invalid = False
            self._draw()

    def _draw(self) -> None:
        self.canvas.begin_frame()
        self._call("on_draw", self._ctx, self.canvas.g)
        self.frame_commands = self.canvas.end_frame()

    def _overlay_items(self) -> list[dict]:
        if not self._overlay:
            return []
        title, msg = self._overlay
        w = self.canvas.width
        items, y = [], self.canvas.height // 2 - 60
        for text, font in [(title, "large")] + [(line, "small") for line in self.canvas._wrap("small", msg, w - 64)[:4]]:
            tw, lh = self.fonts.measure(font, text), self.fonts.line_height(font)
            items.append({"text": text, "x": round((w - tw) / 2), "y": y, "w": tw, "h": lh, "font": font})
            y += lh + (8 if font == "large" else 0)
        return items

    # ---- log --------------------------------------------------------------------------------

    def _log_line(self, msg: str) -> None:
        t = self.clock.now()
        self.log.append(f"{t:%H:%M:%S}.{t.microsecond // 1000:03d} {msg}")

    def _warn(self, msg: str) -> None:
        self.warnings.append(msg)
        self._log_line("WARN " + msg)
