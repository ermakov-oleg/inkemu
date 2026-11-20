"""Basic ctx members: api, state, timers, clock, device, fonts. Installed by Emu on every boot."""
import base64
import binascii
import hashlib
import json
import math
import random
import re
import shutil
import uuid
import zlib
from io import BytesIO
from pathlib import Path
from urllib.parse import quote, unquote_to_bytes
from typing import TYPE_CHECKING, Any, Callable

from lupa import lua55

from .fonts import LINE_HEIGHTS
from .lua_env import add_methods, make_object

if TYPE_CHECKING:
    from .emu import Emu

_SKIP = object()
_MAX_DEPTH = 64
MAX_BYTES, MAX_FILES = 134217728, 512
_ESCAPED = re.compile(r"\\x([0-9a-f]{2})")
_BAD_NAME = re.compile(r"[\s/\\\x00-\x1f\x7f]")
CAPABILITIES = {"dialog": True, "frontlight": True, "image_decode": True, "image_transform": True,
                "pattern": True, "reader": True, "ui": False, "ttf": False}
# Lua-side shim: strings with bytes >= 0x80 would fail the utf-8 decode on the way into Python,
# so they travel as \xHH escapes and `raw()` restores them.
_BINARY_SHIM = r"""
local gsub, type, fmt, select, unpack = string.gsub, type, string.format, select, table.unpack
local function esc(s) return (gsub(s, '[\\\128-\255]', function(c) return fmt('\\x%02x', c:byte()) end)) end
return function(w)
  return function(self, ...)
    local n, args = select('#', ...), { ... }
    for i = 1, n do if type(args[i]) == 'string' then args[i] = esc(args[i]) end end
    return w(self, unpack(args, 1, n))
  end
end
"""


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


def lua_to_py(v: Any, warn: Callable[[str], None], path: str = "state", null: Any = None, L=None, _seen: tuple = ()) -> Any:
    """Lua value -> JSON-ready Python: 1..n tables become lists, other tables dicts with string keys.
    Functions, userdata, NaN/inf, cycles and too-deep nesting are skipped with a warning naming the key path.
    Pass the runtime `L` to detect cycles and the `null` sentinel (lupa wrappers only compare by rawequal)."""
    t = lua55.lua_type(v)
    if t is None:
        if isinstance(v, float) and not math.isfinite(v):
            warn(f"{path}: {v} cannot be saved, skipped")
            return _SKIP
        return v
    if t != "table":
        warn(f"{path}: a {t} cannot be saved, skipped")
        return _SKIP
    same = L.globals().rawequal if L is not None else None
    if null is not None and same(v, null):
        return None
    if same and any(same(v, anc) for anc in _seen):
        warn(f"{path}: a cycle, skipped")
        return _SKIP
    if len(_seen) >= _MAX_DEPTH:
        warn(f"{path}: nested deeper than {_MAX_DEPTH} levels, skipped")
        return _SKIP
    out: dict = {}
    try:
        for k, x in v.items():
            val = lua_to_py(x, warn, f"{path}.{k}", null, L, _seen + (v,))
            if val is not _SKIP:
                out[k] = val
    except UnicodeDecodeError:
        warn(f"{path}: contains a string that is not valid UTF-8, skipped")
        return _SKIP
    n = len(out)
    if n and all(isinstance(k, int) and not isinstance(k, bool) for k in out) and set(out) == set(range(1, n + 1)):
        return [out[i] for i in range(1, n + 1)]
    return {(k if isinstance(k, str) else json.dumps(k) if isinstance(k, bool) else str(k)): x for k, x in out.items()}


def py_to_lua(L, v: Any, null: Any = None, _depth: int = 0) -> Any:
    """JSON-style Python value -> Lua; JSON null becomes `null` (nil when None, so keys vanish)."""
    if v is None:
        return null
    if isinstance(v, int) and not isinstance(v, bool) and not -(2 ** 63) <= v < 2 ** 63:
        return float(v)
    if not isinstance(v, (dict, list)):
        return v
    t = L.table()
    if isinstance(v, dict):
        for k, x in v.items():
            if x is not None or null is not None:
                t[str(k)] = py_to_lua(L, x, null, _depth + 1)
    else:
        for i, x in enumerate(v, 1):
            if x is not None or null is not None:
                t[i] = py_to_lua(L, x, null, _depth + 1)
    return t


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
    data = lua_to_py(emu._ctx.state, emu._warn, L=emu.L)
    if data is None:
        data = {}
    if not isinstance(data, (dict, list)):
        emu._warn(f"ctx.state is a {lua_typename(data)}, not a table: not saved")
        return
    emu.state_dir.mkdir(parents=True, exist_ok=True)
    tmp = emu.state_dir / "state.json.tmp"
    tmp.write_text(json.dumps(data, ensure_ascii=False, sort_keys=True), encoding="utf-8")
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
    _install_files(emu)
    _install_package(emu)
    _install_utils(emu)
    _install_screen(emu)
    _install_stubs(emu)
    if "reader" in (emu.manifest.get("permissions") or []):
        _install_reader(emu)


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


def raw(v: Any) -> bytes | None:
    """A string argument that came through the binary shim, as bytes; numbers are stringified, others None."""
    if isinstance(v, bytes):
        return v
    if isinstance(v, str):
        return _ESCAPED.sub(lambda m: chr(int(m[1], 16)), v).encode("latin-1")
    if isinstance(v, (int, float)) and not isinstance(v, bool):
        return _fmt(v).encode()
    return None


def _bytes_arg(fn: str, i: int, v: Any) -> bytes:
    b = raw(v)
    if b is None:
        raise TypeError(f"bad argument #{i} to '{fn}' (string expected, got {lua_typename(v)})")
    return b


def _binary_object(L, name: str, methods: dict[str, Callable], fields: dict | None = None):
    """make_object whose methods accept arbitrary bytes in string arguments."""
    if not hasattr(L, "_binary"):
        L._binary = L.execute(_BINARY_SHIM)
    t = make_object(L, name, methods, fields)
    for m in methods:
        t[m] = L._binary(t[m])
    return t


def _valid_name(b: bytes | None) -> str | None:
    try:
        name = b.decode("utf-8") if b is not None else ""
    except UnicodeDecodeError:
        return None
    if not name or name in (".", "..") or len(b) > 255 or _BAD_NAME.search(name):
        return None
    return name


def seed_files(emu: "Emu") -> None:
    """Copy <fixtures_dir>/files/* into the app's files folder, never overwriting."""
    src = emu.fixtures_dir / "files"
    if not src.is_dir():
        return
    dst = emu.state_dir / "files"
    for f in sorted(src.iterdir()):
        if f.is_file() and not f.is_symlink() and _valid_name(f.name.encode()) and not (dst / f.name).exists():
            dst.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(f, dst / f.name)


def _install_files(emu: "Emu") -> None:
    root = emu.state_dir / "files"

    def entries() -> list[Path]:
        return [p for p in root.iterdir() if p.is_file() and _valid_name(p.name.encode())] if root.is_dir() else []

    def target(fn: str, name) -> Path | tuple:
        n = _valid_name(raw(name))
        return root / n if n else (None, "invalid_argument")

    def write(name=None, data=None, *_):
        p = target("write", name)
        if isinstance(p, tuple):
            return p
        body = raw(data)
        if body is None:
            return None, "invalid_argument"
        others = [e for e in entries() if e.name != p.name]
        if len(body) + sum(e.stat().st_size for e in others) > MAX_BYTES or len(others) + 1 > MAX_FILES:
            return None, "no_space"
        root.mkdir(parents=True, exist_ok=True)
        tmp = root / (p.name + ".inkemu-tmp")
        tmp.write_bytes(body)
        tmp.replace(p)
        return True

    def read(name=None, *extra):
        p = target("read", name)
        if isinstance(p, tuple):
            return p
        if any(x is not None for x in extra):
            emu._warn("storage:read: offset/length ignored; the device always returns the whole file")
        return p.read_bytes() if p.is_file() else (None, "not_found")

    def exists(name=None, *_):
        p = target("exists", name)
        return False if isinstance(p, tuple) else p.is_file()

    def size(name=None, *_):
        p = target("size", name)
        if isinstance(p, tuple):
            return p
        return p.stat().st_size if p.is_file() else (None, "not_found")

    def remove(name=None, *_):
        p = target("remove", name)
        if isinstance(p, tuple):
            return p
        if not p.is_file():
            return None, "not_found"
        p.unlink()
        return True

    def usage(*_):
        es = entries()
        return emu.L.table_from({"used_bytes": sum(e.stat().st_size for e in es), "used_files": len(es),
                                 "max_bytes": MAX_BYTES, "max_files": MAX_FILES})

    methods = {"write": write, "read": read, "exists": exists, "size": size, "remove": remove, "usage": usage,
               "list": lambda *a: emu.L.table_from(sorted(e.name for e in entries()))}
    emu._ctx.storage = _binary_object(emu.L, "storage", methods)
    emu._ctx.files = _binary_object(emu.L, "files", methods, {"path": f"/AppFiles/{emu.app_id}"})


def _install_package(emu: "Emu") -> None:
    L, root = emu.L, emu.app_dir / "data"

    def find(name) -> Path | tuple:
        b = raw(name)
        try:
            p = (root / b.decode("utf-8")).resolve() if b else None
        except (UnicodeDecodeError, ValueError):
            p = None
        if p is None or not p.is_relative_to(root.resolve()):
            return None, "invalid_argument"
        return p if p.is_file() else (None, "not_found")

    def read(name=None, *_):
        p = find(name)
        return p if isinstance(p, tuple) else p.read_bytes()

    def open_text(name=None, *_):
        p = find(name)
        if isinstance(p, tuple):
            return p
        buf = BytesIO(p.read_bytes())

        def read_line(*_):
            line = buf.readline()
            return line[:-1] if line.endswith(b"\n") else (line or None)

        def seek(pos=0, *_):
            buf.seek(int(pos))
            return True

        return make_object(L, "text", {"read_line": read_line, "seek": seek, "tell": lambda *a: buf.tell(),
                                       "close": lambda *a: True})

    def image(name=None, *_):
        p = find(name)
        if isinstance(p, tuple):
            return p
        decoder = emu._ctx.image
        if decoder is None:
            return None, "unsupported"
        return decoder["decode"](decoder, p.read_bytes())

    emu._ctx.package = _binary_object(L, "package", {
        "read": read, "open_text": open_text, "image": image,
        "exists": lambda name=None, *_: not isinstance(find(name), tuple),
        "size": lambda name=None, *_: (lambda p: p if isinstance(p, tuple) else p.stat().st_size)(find(name)),
    })


def _install_utils(emu: "Emu") -> None:
    L = emu.L
    null = L.execute("return setmetatable({}, {__tostring = function() return 'null' end, __metatable = false})")
    rng = random.Random(f"inkemu:{emu.app_id}")

    def decode(s=None, *_):
        text = _bytes_arg("decode", 1, s)
        try:
            return py_to_lua(L, json.loads(text.decode("utf-8"), parse_constant=_no_constant), null)
        except (ValueError, RecursionError):
            return None, "invalid_argument"

    def encode(v=None, *_):
        data = lua_to_py(v, emu._warn, "json", null, L)
        if data is _SKIP:
            return None, "invalid_argument"
        return json.dumps(data, ensure_ascii=False, sort_keys=True, separators=(",", ":"))

    def codec(fn: str, f: Callable[[bytes], Any], bad=(ValueError,)):
        def run(s=None, *_):
            try:
                return f(_bytes_arg(fn, 1, s))
            except bad:
                return None, "invalid_argument"
        return run

    def decompress(b: bytes) -> bytes:
        try:
            return zlib.decompress(b, 47)
        except zlib.error:
            return zlib.decompress(b, -15)

    def unhex(b: bytes) -> bytes:
        if len(b) % 2:
            raise ValueError("odd length")
        return binascii.unhexlify(b)

    ctx = emu._ctx
    ctx.json = _binary_object(L, "json", {"decode": decode, "encode": encode}, {"null": null})
    ctx.base64 = _binary_object(L, "base64", {
        "encode": codec("encode", lambda b: base64.b64encode(b).decode()),
        "decode": codec("decode", lambda b: base64.b64decode(b, validate=True), (ValueError, binascii.Error))})
    ctx.hex = _binary_object(L, "hex", {
        "encode": codec("encode", lambda b: b.hex()),
        "decode": codec("decode", unhex, (ValueError, binascii.Error))})
    ctx.hash = _binary_object(L, "hash", {
        "sha1": codec("sha1", lambda b: hashlib.sha1(b).hexdigest()),
        "sha256": codec("sha256", lambda b: hashlib.sha256(b).hexdigest())})
    ctx.url = _binary_object(L, "url", {
        "encode": codec("encode", lambda b: quote(b, safe="-_.~")),
        "decode": codec("decode", unquote_to_bytes)})
    ctx.zlib = _binary_object(L, "zlib", {"decompress": codec("decompress", decompress, (zlib.error,))})
    ctx.random = make_object(L, "random", {"uuid": lambda *a: str(uuid.UUID(int=rng.getrandbits(128), version=4))})


def _no_constant(name: str):
    raise ValueError(f"{name} is not valid JSON")


def _install_screen(emu: "Emu") -> None:
    L = emu.L
    sizes = {"portrait": (480, 800), "landscape": (800, 480)}

    def orientation() -> str:
        return "landscape" if emu.canvas.width > emu.canvas.height else "portrait"

    screen = make_object(L, "screen", {}, {"width": emu.canvas.width, "height": emu.canvas.height,
                                           "orientation": orientation()})

    def set_orientation(name=None, *_):
        if name not in sizes:
            return None, "invalid_argument"
        if name != orientation():
            emu.canvas.resize(*sizes[name])
            screen["width"], screen["height"], screen["orientation"] = *sizes[name], name
            emu.invalidate()
        return True

    add_methods(L, screen, "screen", {"set_orientation": set_orientation})
    emu._ctx.screen = screen


def _install_stubs(emu: "Emu") -> None:
    L, ctx = emu.L, emu._ctx

    def count(kind: str):
        def run(*_):
            emu.refresh_counts[kind] += 1
        return run

    def set_light(n=None, *_):
        check_arg("set", 1, n, "number")
        emu.frontlight = int(n)
        return True

    def open_url(target=None, *_):
        emu._log_line(f"system:open {_fmt(target)}")
        return False

    ctx.capabilities = L.table_from(CAPABILITIES)
    ctx.i18n = L.table_from({"locale": emu.locale})
    ctx.display = make_object(L, "display", {"request_refresh": count("request"), "full_refresh": count("full")})
    ctx.frontlight = make_object(L, "frontlight", {"get": lambda *a: emu.frontlight, "set": set_light})
    ctx.system = make_object(L, "system", {"can_open": lambda *a: False, "open": open_url})
    ctx.perf = make_object(L, "perf", {"info": lambda *a: L.table_from(
        {"free_heap": 1048576, "min_free_heap": 524288, "uptime_ms": emu.clock.ms})})
    ctx.gc = make_object(L, "gc", {"collect": lambda *a: None, "count": lambda *a: 256.0})
    ctx.budget = make_object(L, "budget", {"extend": lambda *a: True})


def _install_reader(emu: "Emu") -> None:
    L = emu.L

    def data() -> dict:
        try:
            d = json.loads((emu.fixtures_dir / "reader.json").read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return {}
        return d if isinstance(d, dict) else {}

    def part(key: str, default):
        def run(*_):
            v = data().get(key, default)
            return None if v is None else py_to_lua(L, v)
        return run

    emu._ctx.reader = make_object(L, "reader", {
        "summary": part("summary", {"streak": 0, "books": 0, "seconds": 0}),
        "books": part("books", []), "all_books": part("all_books", []), "current": part("current", None)})
