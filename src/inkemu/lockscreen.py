"""Lock screen ctx: `ctx.lock` inside the lock state, `ctx.lockscreen` in the app (manifest block required)."""
import json
from typing import TYPE_CHECKING

from .ctx_basic import check_arg
from .lua_env import make_object

if TYPE_CHECKING:
    from .emu import Emu


def _load(emu: "Emu") -> dict:
    try:
        data = json.loads((emu.state_dir / "lock.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def _store(emu: "Emu", key: str, value: bool) -> None:
    data = _load(emu)
    data[key] = value
    emu.state_dir.mkdir(parents=True, exist_ok=True)
    (emu.state_dir / "lock.json").write_text(json.dumps(data, sort_keys=True), encoding="utf-8")


def install(emu: "Emu") -> None:
    if emu._lock_ctx is not None:
        _install_lock(emu)
    elif isinstance(emu.manifest.get("lockscreen"), dict):
        _install_api(emu)


def _install_lock(emu: "Emu") -> None:
    st = emu._lock_ctx

    def done(*_):
        st["done"] = True

    def set_interval(sec=None, *_):
        check_arg("set_interval", 1, sec, "number")
        if not sec > 0:
            return None, "invalid_argument"
        emu._set_lock_interval(sec)
        emu._ctx.lock["interval_sec"] = sec
        return True

    emu._ctx.lock = make_object(emu.L, "lock", {"done": done, "set_interval": set_interval}, fields={
        "reason": st["reason"], "net": st["net"], "interval_sec": emu._lock_interval,
        "elapsed_sec": st["elapsed"]})


def _install_api(emu: "Emu") -> None:
    def setter(key: str, value: bool, what: str):
        def run(*_):
            _store(emu, key, value)
            emu._log_line(f"lockscreen: {what}" + (" (auto-confirmed)" if value else ""))
            return True
        return run

    def getter(key: str):
        return lambda *a: bool(_load(emu).get(key, False))

    emu._ctx.lockscreen = make_object(emu.L, "lockscreen", {
        "set_default": setter("is_default", True, "set_default"),
        "clear_default": setter("is_default", False, "clear_default"),
        "is_default": getter("is_default"),
        "set_wallpaper": setter("is_wallpaper", True, "set_wallpaper"),
        "clear_wallpaper": setter("is_wallpaper", False, "clear_wallpaper"),
        "is_wallpaper": getter("is_wallpaper"),
    })
