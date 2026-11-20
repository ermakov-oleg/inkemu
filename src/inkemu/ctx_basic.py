"""Basic ctx members: api, state, timers, clock, device, fonts. Installed by Emu on every boot."""
import hashlib
import json
from pathlib import Path
from typing import TYPE_CHECKING, Any, Callable

from lupa import lua55

from .fonts import LINE_HEIGHTS
from .lua_env import add_methods, make_object

if TYPE_CHECKING:
    from .emu import Emu

_SKIP = object()
_MAX_DEPTH = 64


def lua_typename(v: Any) -> str:
    if v is None:
        return "nil"
    if isinstance(v, bool):
        return "boolean"
    if isinstance(v, (int, float)):
        return "number"
    if isinstance(v, (str, bytes)):
        return "string"
    return lua55.lua_type(v) or "userdata"


def check_arg(fn: str, i: int, v: Any, expected: str) -> None:
    if lua_typename(v) != expected:
        raise TypeError(f"bad argument #{i} to '{fn}' ({expected} expected, got {lua_typename(v)})")


def lua_to_py(v: Any, warn: Callable[[str], None], path: str = "state", _depth: int = 0) -> Any:
    """Lua value -> JSON-ready Python: 1..n tables become lists, other tables dicts with string keys.
    Functions, userdata and too-deep nesting are skipped with a warning."""
    t = lua55.lua_type(v)
    if t is None:
        return v
    if t != "table":
        warn(f"{path}: a {t} cannot be saved, skipped")
        return _SKIP
    if _depth >= _MAX_DEPTH:
        warn(f"{path}: nested deeper than {_MAX_DEPTH} levels (a cycle?), skipped")
        return _SKIP
    out: dict = {}
    for k, x in v.items():
        val = lua_to_py(x, warn, f"{path}.{k}", _depth + 1)
        if val is not _SKIP:
            out[k] = val
    n = len(out)
    if n and all(isinstance(k, int) and not isinstance(k, bool) for k in out) and set(out) == set(range(1, n + 1)):
        return [out[i] for i in range(1, n + 1)]
    return {(k if isinstance(k, str) else json.dumps(k) if isinstance(k, bool) else str(k)): x for k, x in out.items()}


def py_to_lua(L, v: Any) -> Any:
    return L.table_from(v, recursive=True) if isinstance(v, (dict, list)) else v


def load_state(path: Path, warn: Callable[[str], None]) -> dict | list:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {}
    except (OSError, ValueError) as e:
        warn(f"{path}: unreadable state ({e}), starting with an empty ctx.state")
        return {}
    if not isinstance(data, (dict, list)):
        warn(f"{path}: state is not a table, starting with an empty ctx.state")
        return {}
    return data


def save_state(emu: "Emu") -> None:
    if emu._ctx is None:
        return
    data = lua_to_py(emu._ctx.state, emu._warn)
    if data is None:
        data = {}
    if not isinstance(data, (dict, list)):
        emu._warn(f"ctx.state is a {lua_typename(data)}, not a table: not saved")
        return
    emu.state_dir.mkdir(parents=True, exist_ok=True)
    tmp = emu.state_dir / "state.json.tmp"
    tmp.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
    tmp.replace(emu.state_dir / "state.json")


def _fmt(v: Any) -> str:
    if v is None:
        return "nil"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float):
        return repr(v)
    if isinstance(v, (str, int)):
        return str(v)
    if isinstance(v, bytes):
        return v.decode("utf-8", "replace")
    return lua_typename(v)


def install(emu: "Emu") -> None:
    L, ctx = emu.L, emu._ctx

    def after(ms=None, fn=None, *_):
        check_arg("after", 1, ms, "number")
        check_arg("after", 2, fn, "function")
        timer = emu._add_timer(round(ms), lambda: emu._protected(fn, ctx, where="ctx:after callback"))
        return make_object(L, "timer", {"cancel": lambda *a: emu._cancel_timer(timer)})

    def set_tick(ms=None, *_):
        check_arg("set_tick", 1, ms, "number")
        emu._set_tick(round(ms))

    def defer_full_refresh(n=None, *_):
        check_arg("defer_full_refresh", 1, n, "number")
        emu.full_refresh_deferred = int(n)

    add_methods(L, ctx, "ctx", {
        "invalidate": lambda *a: emu.invalidate(),
        "defer_full_refresh": defer_full_refresh,
        "after": after,
        "set_tick": set_tick,
        "quit": lambda *a: emu._request_quit(),
        "log": lambda *a: emu._log_line("[app] " + " ".join(_fmt(x) for x in a)),
        "save_state": lambda *a: emu.save_state(),
    })
    major, minor = emu.firmware_api
    ctx.api = L.table_from({"major": major, "minor": minor})
    ctx.state = py_to_lua(L, load_state(emu.state_dir / "state.json", emu._warn))
    _install_clock(emu)
    _install_device(emu)
    _install_fonts(emu)


def _install_clock(emu: "Emu") -> None:
    L = emu.L

    def now(*_):
        dt = emu.clock.now()
        epoch = int(dt.timestamp())
        offset = int(dt.astimezone().utcoffset().total_seconds())
        return L.table_from({
            "year": dt.year, "month": dt.month, "day": dt.day, "hour": dt.hour, "minute": dt.minute,
            "second": dt.second, "wday": dt.isoweekday() % 7 + 1, "yday": dt.timetuple().tm_yday,
            "epoch_sec": epoch, "local_sec": epoch + offset, "valid": True, "hour12": False, "date_order": "dmy",
        })

    emu._ctx.clock = make_object(L, "clock", {"now": now, "millis": lambda *a: emu.clock.ms})


def _install_device(emu: "Emu") -> None:
    major, minor = emu.firmware_api
    emu._ctx.device = make_object(emu.L, "device", {"battery": lambda *a: emu.L.table_from(dict(emu.battery))}, fields={
        "firmware": f"inkemu (API {major}.{minor})",
        "model": "X4 Pro",
        "id_for_app": hashlib.sha256(f"inkemu:{emu.app_id}".encode()).hexdigest()[:16],
    })


def _install_fonts(emu: "Emu") -> None:
    L, fonts = emu.L, emu.fonts
    handles = {}
    for name, lh in LINE_HEIGHTS.items():
        def measure(s=None, *_, _name=name):
            return fonts.measure(_name, _fmt(s))
        handles[name] = make_object(L, f"fonts.{name}", {"measure": measure},
                                    fields={"name": name, "line_height": lh, "ascent": fonts.pil(name).getmetrics()[0]})

    def size(px=None, *_):
        check_arg("size", 1, px, "number")
        return handles[min(LINE_HEIGHTS, key=lambda n: abs(LINE_HEIGHTS[n] - px))]

    table = make_object(L, "fonts", {"size": size})
    for name, h in handles.items():
        table[name] = h
    emu._ctx.fonts = table
