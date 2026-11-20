"""stdio MCP server: starts/finds the inkemu daemon and forwards commands to it over HTTP."""

import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from mcp.server.mcpserver import Image, MCPServer

from .daemon import DaemonUnreachable, post_command, read_info
from .emu import NET_MODES
from .paths import app_id_of, state_root

START_TIMEOUT_S = 20
mcp = MCPServer("inkemu")


def _post(cmd: str, args: dict | None = None) -> dict:
    info = read_info()
    if not info:
        return {"ok": False, "error": "no emulator running; call emu_start first"}
    try:
        return post_command(int(info["port"]), cmd, args)
    except DaemonUnreachable as e:
        return {"ok": False, "error": str(e)}


def _attach(live: dict, window: bool | None, net: str | None, fresh: bool) -> dict:
    """State of the running daemon for the same app, or ok=false if it cannot give what was asked for."""
    state = _post("state")
    have = {"net": (state.get("status") or {}).get("net"), "window": bool(live.get("window"))}
    asked = {k: v for k, v in (("net", net), ("window", window)) if v is not None and v != have[k]}
    if fresh:
        asked["fresh"] = True
    if asked:
        want = ", ".join(f"{k}={v}" for k, v in asked.items())
        now = ", ".join(f"{k}={v}" for k, v in have.items())
        return {
            **state,
            "ok": False,
            "already_running": True,
            "error": f"a daemon for this app is already running ({now}) and cannot switch to {want}; "
            "call emu_stop first, then emu_start again",
        }
    return {**state, "already_running": True}


@mcp.tool()
def emu_start(app: str, window: bool | None = None, net: str | None = None, fresh: bool = False, port: int = 0) -> dict:
    """Start the emulator daemon for an app directory: headless by default, or with a window; net replay (default),
    record or live. Reuses a running daemon for the same app unless window/net/fresh ask for something else.
    port 0 picks a free port."""
    if net is not None and net not in NET_MODES:
        return {"ok": False, "error": f"net must be one of {', '.join(NET_MODES)}, not {net!r}"}
    if live := read_info():
        if live["app"] != app_id_of(app):
            return {
                "ok": False,
                "error": f"a daemon for app {live['app']!r} is already running (pid {live['pid']}); "
                "call emu_stop first",
            }
        return _attach(live, window, net, fresh)
    cmd = [
        sys.executable,
        "-m",
        "inkemu",
        "window" if window else "serve",
        app,
        "--net",
        net or "replay",
        "--port",
        str(port),
    ]
    if fresh:
        cmd.append("--fresh")
    root = state_root()
    root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryFile() as err:
        proc = subprocess.Popen(
            cmd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=err,
            start_new_session=True,
            env=os.environ.copy(),
        )
        deadline = time.monotonic() + START_TIMEOUT_S
        while time.monotonic() < deadline:
            info = read_info()
            if info and info["pid"] == proc.pid:
                return {**_post("state"), "pid": proc.pid, "port": info["port"]}
            if proc.poll() is not None:
                break
            time.sleep(0.1)
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        err.seek(0)
        return {"ok": False, "error": "the daemon did not start: " + err.read().decode("utf-8", "replace")[-2000:]}


@mcp.tool()
def emu_tap(x: int, y: int) -> dict:
    """Tap at screen coordinates."""
    return _post("tap", {"x": x, "y": y})


@mcp.tool()
def emu_tap_text(text: str) -> dict:
    """Tap the centre of the visible text (the last drawn one if it occurs several times)."""
    return _post("tap-text", {"text": text})


@mcp.tool()
def emu_double_tap(x: int, y: int) -> dict:
    """Double tap at screen coordinates."""
    return _post("double-tap", {"x": x, "y": y})


@mcp.tool()
def emu_swipe(direction: str) -> dict:
    """Swipe: up, down, left or right."""
    return _post("swipe", {"direction": direction})


@mcp.tool()
def emu_hold(x: int, y: int, ms: int = 800) -> dict:
    """Press and hold at coordinates for ms milliseconds."""
    return _post("hold", {"x": x, "y": y, "ms": ms})


@mcp.tool()
def emu_key(key: str) -> dict:
    """Press a side button or key: up, down, left, right, ok, back."""
    return _post("key", {"key": key})


@mcp.tool()
def emu_type(text: str) -> dict:
    """Type text into the open input dialog."""
    return _post("type", {"text": text})


@mcp.tool()
def emu_answer(ok: bool) -> dict:
    """Answer a confirm dialog: true = yes, false = no."""
    return _post("answer", {"answer": ok})


@mcp.tool()
def emu_wait(duration: str) -> dict:
    """Advance virtual time, e.g. '500ms', '61s', '2m'."""
    return _post("wait", {"duration": duration})


@mcp.tool()
def emu_lock() -> dict:
    """Lock the device (lock screen)."""
    return _post("lock")


@mcp.tool()
def emu_unlock() -> dict:
    """Unlock the device."""
    return _post("unlock")


@mcp.tool()
def emu_reload() -> dict:
    """Reload the app code, keeping its state."""
    return _post("reload")


@mcp.tool()
def emu_restart() -> dict:
    """Restart the app from scratch."""
    return _post("restart")


@mcp.tool()
def emu_screenshot() -> Image:
    """Screenshot of the current screen as a PNG image."""
    fd, tmp = tempfile.mkstemp(prefix="inkemu-shot-", suffix=".png")
    os.close(fd)
    try:
        result = _post("shot", {"path": tmp})
        if not result.get("ok") or not result.get("shot"):
            raise RuntimeError(result.get("error") or "no screenshot")
        return Image(data=Path(tmp).read_bytes(), format="png")
    finally:
        Path(tmp).unlink(missing_ok=True)


@mcp.tool()
def emu_texts() -> dict:
    """Visible texts with coordinates, plus errors, warnings and status."""
    return _post("texts")


@mcp.tool()
def emu_log() -> dict:
    """The full emulator log."""
    return _post("log")


@mcp.tool()
def emu_resume() -> dict:
    """Let the virtual clock run again after remote commands froze it."""
    return _post("resume")


@mcp.tool()
def emu_stop() -> dict:
    """Stop the daemon (and its window)."""
    pid = (read_info() or {}).get("pid")
    result = _post("stop")
    if result.get("ok") and pid:
        for _ in range(100):
            if not read_info():
                break
            time.sleep(0.1)
    return result


def main() -> None:
    mcp.run()
