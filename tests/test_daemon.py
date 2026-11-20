import json
import os
import threading
import time
import urllib.request
from pathlib import Path

import pytest

from inkemu import daemon
from inkemu.daemon import Daemon, UsageError, execute, parse_words, read_info, write_info

HELLO = Path(__file__).resolve().parent.parent / "examples" / "hello"


def test_execute_tap_text_hello(emu_factory):
    emu = emu_factory(HELLO)
    r = execute(emu, "tap-text", {"text": "Tap me"})
    assert r["ok"] and r["error"] is None
    assert any(t["text"] == "Taps: 1" for t in r["texts"])
    assert r["status"]["paused"] is True and r["shot"] is None
    assert set(r) >= {"ok", "error", "texts", "log_tail", "errors", "warnings", "status", "shot"}


def test_execute_unknown_command(emu_factory):
    r = execute(emu_factory(HELLO), "jump", {})
    assert r["ok"] is False and "tap-text" in r["error"]


def test_execute_bad_args_and_missing_text(emu_factory):
    emu = emu_factory(HELLO)
    r = execute(emu, "tap", {"x": "a", "y": 1})
    assert not r["ok"] and "integer" in r["error"]
    r = execute(emu, "tap-text", {"text": "nope"})
    assert not r["ok"] and "no text" in r["error"] and r["texts"]


def test_execute_shot_default_and_path(emu_factory, tmp_path):
    emu = emu_factory(HELLO)
    r1 = execute(emu, "shot", {})
    r2 = execute(emu, "shot", {})
    assert Path(r1["shot"]).exists() and Path(r2["shot"]).exists() and r1["shot"] != r2["shot"]
    assert Path(r1["shot"]).parent == emu.state_dir / "shots"
    target = tmp_path / "sub" / "a.png"
    assert execute(emu, "shot", {"path": str(target)})["shot"] == str(target) and target.exists()


def test_execute_shot_oserror_is_clean(emu_factory, tmp_path):
    blocker = tmp_path / "file"
    blocker.write_text("x")
    r = execute(emu_factory(HELLO), "shot", {"path": str(blocker / "a.png")})
    assert r["ok"] is False and r["shot"] is None and r["error"]


def test_execute_resume_clears_pause(emu_factory):
    emu = emu_factory(HELLO)
    assert execute(emu, "texts", {})["status"]["paused"] is True
    assert execute(emu, "resume", {})["status"]["paused"] is False


@pytest.mark.allow_errors
def test_execute_wait_returns_for_zero_delay_loop(emu_factory, make_app):
    app = make_app("function on_load(c) local f; f = function() c:after(0, f) end; f() end")
    r = execute(emu_factory(app), "wait", {"duration": "1m"})
    assert r["ok"] and any("same virtual instant" in e for e in r["errors"])


def test_parse_words():
    assert parse_words(["tap", "1", "2"]) == ("tap", {"x": 1, "y": 2})
    assert parse_words(["hold", "1", "2"]) == ("hold", {"x": 1, "y": 2})
    assert parse_words(["shot"]) == ("shot", {})
    with pytest.raises(UsageError):
        parse_words(["tap", "1"])
    with pytest.raises(UsageError):
        parse_words([])


def post(port, cmd, args=None):
    req = urllib.request.Request(f"http://127.0.0.1:{port}/cmd", method="POST",
                                 headers={"Content-Type": "application/json"}, data=json.dumps({"cmd": cmd, "args": args or {}}).encode())
    with urllib.request.urlopen(req, timeout=10) as resp:
        return resp.status, json.load(resp)


def test_http_roundtrip(emu_factory):
    emu = emu_factory(HELLO)
    d = Daemon(emu, port=0)
    stop = threading.Event()

    def loop():
        while not stop.is_set():
            d.pump()
            time.sleep(0.005)

    t = threading.Thread(target=loop, daemon=True)
    t.start()
    try:
        status, body = post(d.port, "tap-text", {"text": "Tap me"})
        assert status == 200 and body["ok"] and body["status"]["paused"] is True
        assert any(x["text"] == "Taps: 1" for x in body["texts"])
        status, body = post(d.port, "stop")
        assert body["ok"] and d.stopped
        with pytest.raises(urllib.error.HTTPError) as e:
            urllib.request.urlopen(urllib.request.Request(f"http://127.0.0.1:{d.port}/cmd", method="POST",
                                                          headers={"Content-Type": "application/json"},
                                                          data=b"not json"), timeout=5)
        assert e.value.code == 400
    finally:
        stop.set()
        t.join()
        d.close()


def test_stale_daemon_json_ignored(tmp_path):
    (tmp_path / "daemon.json").write_text(json.dumps({"port": 7480, "pid": 999999, "app": "x"}))
    assert read_info(tmp_path) is None
    (tmp_path / "daemon.json").write_text("garbage")
    assert read_info(tmp_path) is None
    assert read_info(tmp_path / "missing") is None


def test_daemon_json_live_roundtrip(tmp_path):
    write_info(tmp_path, 1234, "hello")
    assert read_info(tmp_path) == {"port": 1234, "pid": os.getpid(), "app": "hello"}
    daemon.remove_info(tmp_path)
    assert read_info(tmp_path) is None


def test_port_in_use_clear_error(emu_factory):
    d1 = Daemon(emu_factory(HELLO), port=0)
    try:
        with pytest.raises(OSError) as e:
            Daemon(emu_factory(HELLO), port=d1.port)
        assert str(d1.port) in str(e.value)
    finally:
        d1.close()


def test_serve_headless_stops_and_cleans_up(emu_factory, tmp_path, monkeypatch):
    monkeypatch.setenv("INKEMU_STATE_ROOT", str(tmp_path / "root"))
    emu = emu_factory(HELLO)
    result = {}

    def run():
        daemon.serve(emu, 0, window=False)
        result["done"] = True

    t = threading.Thread(target=run, daemon=True)
    t.start()
    deadline = time.time() + 10
    info = None
    while time.time() < deadline and not info:
        info = read_info(tmp_path / "root")
        time.sleep(0.02)
    assert info and info["app"] == emu.app_id
    assert post(info["port"], "stop")[1]["ok"]
    t.join(10)
    assert result.get("done") and read_info(tmp_path / "root") is None


def raw(port, method="POST", path="/cmd", headers=None, body=b'{"cmd": "tap-text", "args": {"text": "Tap me"}}'):
    import http.client
    c = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    try:
        c.putrequest(method, path, skip_host=True)
        for k, v in {"Host": f"127.0.0.1:{port}", "Content-Type": "application/json", **(headers or {})}.items():
            if v is not None:
                c.putheader(k, v)
        c.putheader("Content-Length", str(len(body)))
        c.endheaders(body)
        r = c.getresponse()
        return r.status, json.loads(r.read() or b"{}")
    finally:
        c.close()


@pytest.mark.parametrize("headers", [
    {"Content-Type": "text/plain"},
    {"Content-Type": None},
    {"Host": "evil.example:7480"},
    {"Host": "127.0.0.1"},
    {"Origin": "http://evil.example"},
])
def test_browser_style_requests_are_rejected(emu_factory, headers):
    emu = emu_factory(HELLO)
    d = Daemon(emu, port=0)
    try:
        status, body = raw(d.port, headers=headers)
        assert status == 403 and body["ok"] is False and body["error"]
        assert d.queue.empty() and emu.has_text("Taps: 0")
    finally:
        d.close()


def test_other_methods_and_paths_rejected_cleanly(emu_factory):
    d = Daemon(emu_factory(HELLO), port=0)
    try:
        assert raw(d.port, "GET", body=b"")[0] == 405
        assert raw(d.port, "PUT")[0] == 405
        assert raw(d.port, path="/other")[0] == 404
        assert d.queue.empty()
    finally:
        d.close()
