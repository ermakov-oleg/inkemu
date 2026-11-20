"""MCP tool functions called in-process against an in-process daemon (the stdio round trip is in test_mcp)."""

import threading
import time
from pathlib import Path

import pytest

from inkemu import daemon, mcp_server

HELLO = Path(__file__).resolve().parent.parent / "examples" / "hello"


@pytest.fixture
def live_daemon(tmp_path, monkeypatch, emu_factory):
    monkeypatch.setenv("INKEMU_STATE_ROOT", str(tmp_path / "root"))
    emu = emu_factory(HELLO)
    t = threading.Thread(target=daemon.serve, args=(emu, 0, False), daemon=True)
    t.start()
    for _ in range(500):
        if daemon.read_info(tmp_path / "root"):
            break
        time.sleep(0.02)
    yield emu
    mcp_server.emu_stop()
    t.join(10)
    assert daemon.read_info(tmp_path / "root") is None


def test_emu_start_attaches_when_nothing_differs(live_daemon):
    r = mcp_server.emu_start(str(HELLO))
    assert r["ok"] and r["already_running"]
    assert mcp_server.emu_start(str(HELLO), net="replay", window=False)["ok"]


@pytest.mark.parametrize(
    "kw, word", [({"net": "record"}, "net=record"), ({"window": True}, "window=True"), ({"fresh": True}, "fresh=True")]
)
def test_emu_start_refuses_a_different_mode(live_daemon, kw, word):
    r = mcp_server.emu_start(str(HELLO), **kw)
    assert not r["ok"] and word in r["error"] and "emu_stop" in r["error"]


def test_emu_start_rejects_unknown_net(live_daemon):
    assert "net must be one of" in mcp_server.emu_start(str(HELLO), net="wifi")["error"]


def test_emu_screenshot_leaves_no_files(live_daemon, tmp_path, monkeypatch):
    import tempfile

    tmp = tmp_path / "tmp"
    tmp.mkdir()
    monkeypatch.setattr(tempfile, "tempdir", str(tmp))
    img = mcp_server.emu_screenshot()
    assert img.data[:8] == b"\x89PNG\r\n\x1a\n"
    assert list(tmp.iterdir()) == []
    shots = live_daemon.state_dir / "shots"
    assert not shots.exists() or list(shots.iterdir()) == []
