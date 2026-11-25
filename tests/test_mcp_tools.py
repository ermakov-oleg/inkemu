"""The MCP tools over stdio JSON-RPC against the real `inkemu-mcp` and a real `inkemu serve`."""

import base64
import json
from pathlib import Path

import pytest

HELLO = Path(__file__).resolve().parent.parent / "examples" / "hello"

SCHEMAS = {
    "emu_start": (["app", "window", "net", "fresh", "port"], ["app"]),
    "emu_tap": (["x", "y"], ["x", "y"]),
    "emu_tap_text": (["text"], ["text"]),
    "emu_double_tap": (["x", "y"], ["x", "y"]),
    "emu_swipe": (["direction"], ["direction"]),
    "emu_hold": (["x", "y", "ms"], ["x", "y"]),
    "emu_key": (["key"], ["key"]),
    "emu_type": (["text"], ["text"]),
    "emu_answer": (["ok"], ["ok"]),
    "emu_wait": (["duration"], ["duration"]),
    "emu_lock": ([], []),
    "emu_unlock": ([], []),
    "emu_reload": ([], []),
    "emu_restart": ([], []),
    "emu_screenshot": ([], []),
    "emu_texts": ([], []),
    "emu_log": ([], []),
    "emu_resume": ([], []),
    "emu_stop": ([], []),
}


def test_tools_list_and_schemas(mcp):
    tools = mcp().tools()
    assert sorted(t["name"] for t in tools) == sorted(SCHEMAS)
    for t in tools:
        props, required = SCHEMAS[t["name"]]
        schema = t["inputSchema"]
        assert sorted(schema.get("properties", {})) == sorted(props), t["name"]
        assert schema.get("required", []) == required, t["name"]


def test_emu_start_attaches_when_nothing_differs(serve, mcp):
    m = mcp()
    r = m.payload("emu_start", app=str(HELLO))
    assert r["ok"] and r["already_running"]
    assert m.payload("emu_start", app=str(HELLO), net="replay", window=False)["ok"]


@pytest.mark.parametrize(
    "kw, word", [({"net": "record"}, "net=record"), ({"window": True}, "window=True"), ({"fresh": True}, "fresh=True")]
)
def test_emu_start_refuses_a_different_mode(serve, mcp, kw, word):
    r = mcp().payload("emu_start", app=str(HELLO), **kw)
    assert not r["ok"] and word in r["error"] and "emu_stop" in r["error"]


def test_emu_start_rejects_unknown_net(serve, mcp):
    assert "net must be one of" in mcp().payload("emu_start", app=str(HELLO), net="wifi")["error"]


def test_emu_start_refuses_another_app(serve, mcp, make_app):
    r = mcp().payload("emu_start", app=str(make_app("-- other")))
    assert not r["ok"] and "already running" in r["error"]


def test_emu_screenshot_leaves_no_files(serve, mcp, tmp_path):
    result = mcp().call("emu_screenshot")
    block = result["content"][0]
    assert block["type"] == "image" and block["mimeType"] == "image/png"
    assert base64.b64decode(block["data"])[:8] == b"\x89PNG\r\n\x1a\n"
    assert list((tmp_path / "tmp").iterdir()) == []
    shots = serve.state / "shots"
    assert not shots.exists() or list(shots.iterdir()) == []


def test_tools_without_a_daemon(mcp):
    m = mcp()
    r = m.payload("emu_tap", x=1, y=1)
    assert r["ok"] is False and "emu_start first" in r["error"]
    assert m.call("emu_screenshot")["isError"] is True


def test_emu_stop_stops_the_daemon(serve, mcp):
    assert mcp().payload("emu_stop")["ok"]
    assert serve.wait_exit() == 0
    assert not (serve.root / "daemon.json").exists()


def test_emu_start_spawns_a_daemon(mcp, root):
    m = mcp()
    started = m.payload("emu_start", app=str(HELLO), fresh=True, port=0)
    assert started["ok"], started
    info = json.loads((root / "daemon.json").read_text(encoding="utf-8"))
    assert started["pid"] == info["pid"] and started["port"] == info["port"] and info["port"]
    assert m.payload("emu_tap_text", text="Tap me")["ok"]
    assert any(t["text"] == "Taps: 1" for t in m.payload("emu_texts")["texts"])
    assert m.payload("emu_stop")["ok"]
    assert not (root / "daemon.json").exists()
