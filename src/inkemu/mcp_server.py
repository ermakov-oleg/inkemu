"""stdio MCP server: starts/finds the inkemu daemon and forwards commands to it over HTTP."""
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

from mcp.server.mcpserver import Image, MCPServer

from .daemon import read_info, state_root

START_TIMEOUT_S = 20
mcp = MCPServer("inkemu")


def _app_id(app: str) -> str:
    # same fallback as Emu: untrusted manifest app_id, else letters of the directory name
    import re
    p = Path(app).absolute()
    try:
        app_id = json.loads((p / "manifest.json").read_text(encoding="utf-8")).get("app_id")
    except (OSError, ValueError, AttributeError):
        app_id = None
    if isinstance(app_id, str) and re.fullmatch(r"[A-Za-z]+", app_id):
        return app_id
    return re.sub(r"[^A-Za-z]", "", p.name) or "app"


def _post(cmd: str, args: dict | None = None) -> dict:
    info = read_info()
    if not info:
        return {"ok": False, "error": "no emulator running; call emu_start first"}
    req = urllib.request.Request(f"http://127.0.0.1:{int(info['port'])}/cmd", method="POST",
                                 headers={"Content-Type": "application/json"},
                                 data=json.dumps({"cmd": cmd, "args": args or {}}).encode("utf-8"))
    try:
        with urllib.request.urlopen(req, timeout=620) as resp:
            return json.load(resp)
    except urllib.error.HTTPError as e:
        return json.load(e)
    except (urllib.error.URLError, OSError, ValueError) as e:
        return {"ok": False, "error": f"cannot reach the daemon: {e}"}


@mcp.tool()
def emu_start(app: str, window: bool = False, net: str = "replay", fresh: bool = False, port: int = 0) -> dict:
    """Start the emulator daemon for an app directory (headless, or with a window). port 0 picks a free port."""
    if live := read_info():
        if live["app"] != _app_id(app):
            return {"ok": False, "error": f"a daemon for app {live['app']!r} is already running (pid {live['pid']}); "
                                          "call emu_stop first"}
        return {**_post("state"), "already_running": True}
    cmd = [sys.executable, "-m", "inkemu", "window" if window else "serve", app, "--net", net, "--port", str(port)]
    if fresh:
        cmd.append("--fresh")
    root = state_root()
    root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryFile() as err:
        proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=err,
                                start_new_session=True, env=os.environ.copy())
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
    result = _post("shot")
    if not result.get("ok") or not result.get("shot"):
        raise RuntimeError(result.get("error") or "no screenshot")
    return Image(data=Path(result["shot"]).read_bytes(), format="png")


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
