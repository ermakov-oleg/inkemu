import asyncio
import json
import os
import signal
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

ROOT = Path(__file__).resolve().parent.parent
HELLO = "examples/hello"


def _payload(result) -> dict:
    return json.loads(result.content[0].text)


async def _session(state_root: Path):
    params = StdioServerParameters(command="uv", args=["run", "inkemu-mcp"], cwd=ROOT,
                                   env={**os.environ, "INKEMU_STATE_ROOT": str(state_root)})
    return stdio_client(params), params


async def _scenario(state_root: Path):
    client, _ = await _session(state_root)
    async with client as (r, w):
        async with ClientSession(r, w) as s:
            await s.initialize()
            names = {t.name for t in (await s.list_tools()).tools}
            assert {"emu_start", "emu_tap_text", "emu_screenshot", "emu_texts", "emu_stop"} <= names
            started = _payload(await s.call_tool("emu_start", {"app": HELLO, "fresh": True}))
            assert started["ok"], started
            other = _payload(await s.call_tool("emu_start", {"app": "examples/other"}))
            assert not other["ok"] and "already running" in other["error"]
            tapped = _payload(await s.call_tool("emu_tap_text", {"text": "Tap me"}))
            assert tapped["ok"], tapped
            texts = _payload(await s.call_tool("emu_texts", {}))
            assert any(t["text"] == "Taps: 1" for t in texts["texts"])
            shot = await s.call_tool("emu_screenshot", {})
            assert shot.content[0].type == "image" and shot.content[0].mime_type == "image/png"
            bad = _payload(await s.call_tool("emu_tap_text", {"text": "nope"}))
            assert not bad["ok"] and "Taps: 1" in bad["error"]
            stopped = _payload(await s.call_tool("emu_stop", {}))
            assert stopped["ok"]


def test_mcp_lists_tools_and_screenshot(tmp_path):
    try:
        asyncio.run(asyncio.wait_for(_scenario(tmp_path), 90))
    finally:
        try:
            pid = int(json.loads((tmp_path / "daemon.json").read_text())["pid"])
            os.kill(pid, signal.SIGKILL)
        except (OSError, ValueError, KeyError):
            pass
