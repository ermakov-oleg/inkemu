"""`inkemu serve` and its HTTP protocol, black-box: processes, files and HTTP only."""

import http.client
import json
import signal
import socket
import urllib.request
from pathlib import Path

import pytest

HELLO = Path(__file__).resolve().parent.parent / "examples" / "hello"


def post(port, cmd, args=None):
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}/cmd",
        method="POST",
        headers={"Content-Type": "application/json"},
        data=json.dumps({"cmd": cmd, "args": args or {}}).encode(),
    )
    with urllib.request.urlopen(req, timeout=20) as resp:
        return resp.status, json.load(resp)


def raw(port, method="POST", path="/cmd", headers=None, body=b'{"cmd": "tap-text", "args": {"text": "Tap me"}}'):
    c = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
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


def untouched(port) -> bool:
    return any(t["text"] == "Taps: 0" for t in post(port, "state")[1]["texts"])


def test_http_roundtrip(serve):
    status, body = post(serve.port, "tap-text", {"text": "Tap me"})
    assert status == 200 and body["ok"] is True and body["error"] is None
    assert body["status"]["paused"] is True and body["shot"] is None
    assert any(x["text"] == "Taps: 1" for x in body["texts"])
    assert set(body) >= {"ok", "error", "texts", "log_tail", "errors", "warnings", "status", "shot"}


def test_unknown_command(serve):
    status, r = post(serve.port, "jump")
    assert status == 200 and r["ok"] is False and "tap-text" in r["error"]


def test_bad_args_and_missing_text(serve):
    r = post(serve.port, "tap", {"x": "a", "y": 1})[1]
    assert not r["ok"] and r["error"] == "'tap': x must be an integer, got \"a\""
    r = post(serve.port, "tap-text", {"text": "nope"})[1]
    assert not r["ok"] and "no text" in r["error"] and r["texts"]


def test_shot_default_and_path(serve, tmp_path):
    r1 = post(serve.port, "shot")[1]
    r2 = post(serve.port, "shot")[1]
    assert Path(r1["shot"]).exists() and Path(r2["shot"]).exists() and r1["shot"] != r2["shot"]
    assert Path(r1["shot"]) == serve.state / "shots" / "001.png"
    assert Path(r2["shot"]) == serve.state / "shots" / "002.png"
    target = tmp_path / "sub" / "a.png"
    assert post(serve.port, "shot", {"path": str(target)})[1]["shot"] == str(target) and target.exists()


def test_shot_oserror_is_clean(serve, tmp_path):
    blocker = tmp_path / "file"
    blocker.write_text("x")
    r = post(serve.port, "shot", {"path": str(blocker / "a.png")})[1]
    assert r["ok"] is False and r["shot"] is None and r["error"]


def test_resume_clears_pause(serve):
    assert post(serve.port, "texts")[1]["status"]["paused"] is True
    assert post(serve.port, "resume")[1]["status"]["paused"] is False
    assert post(serve.port, "state")[1]["status"]["paused"] is True


def test_log_and_log_tail(start_serve, make_app):
    s = start_serve(make_app("function on_input(c, ev) c:log('tap') return true end"))
    r = post(s.port, "log")[1]
    assert isinstance(r["log"], list) and r["log"]
    for _ in range(25):
        post(s.port, "tap", {"x": 1, "y": 1})
    r = post(s.port, "log")[1]
    assert len(r["log"]) > 20 and len(r["log_tail"]) == 20


def test_wait_returns_for_zero_delay_loop(start_serve, make_app):
    app = make_app("function on_load(c) local f; f = function() c:after(0, f) end; f() end")
    s = start_serve(app)
    r = post(s.port, "wait", {"duration": "1m"})[1]
    assert r["ok"] and any("same virtual instant" in e for e in r["errors"])


def test_daemon_json_format(serve):
    assert serve.info == {"port": serve.port, "pid": serve.proc.pid, "app": "hello", "window": False}
    assert isinstance(serve.port, int) and serve.port > 0


@pytest.mark.parametrize(
    "headers",
    [
        {"Content-Type": "text/plain"},
        {"Content-Type": None},
        {"Host": "evil.example:{port}"},
        {"Host": "127.0.0.1"},
        {"Origin": "http://evil.example"},
    ],
)
def test_browser_style_requests_are_rejected(serve, headers):
    headers = {k: v.replace("{port}", str(serve.port)) if v else v for k, v in headers.items()}
    status, body = raw(serve.port, headers=headers)
    assert status == 403 and body["ok"] is False and body["error"]
    assert untouched(serve.port)


def test_localhost_host_is_accepted(serve):
    status, body = raw(serve.port, headers={"Host": f"localhost:{serve.port}"})
    assert status == 200 and body["ok"] is True


def test_null_args_count_as_no_args(serve):
    status, reply = raw(serve.port, body=b'{"cmd": "state", "args": null}')
    assert status == 200 and reply["ok"] is True


@pytest.mark.parametrize("args", ["[]", '""', "0", "false", "[1]"])
def test_args_that_are_not_an_object_are_400(serve, args):
    status, reply = raw(serve.port, body=f'{{"cmd": "state", "args": {args}}}'.encode())
    assert status == 400 and reply == {"ok": False, "error": "bad request: args must be an object"}
    assert untouched(serve.port)


@pytest.mark.parametrize(
    "cmd, args, error",
    [
        ("tap", {"x": "5", "y": 1}, "'tap': x must be an integer, got \"5\""),
        ("tap", {"x": 5.0, "y": 1}, "'tap': x must be an integer, got 5.0"),
        ("tap", {"x": True, "y": 1}, "'tap': x must be an integer, got true"),
        ("hold", {"x": 1, "y": 1, "ms": "700"}, "'hold': ms must be an integer, got \"700\""),
        ("type", {"text": 5}, "'type': text must be a string, got 5"),
        (
            "wait",
            {"duration": -1},
            "'wait': duration must be milliseconds or a string like 350ms, 61s, 10m or 2h, got -1",
        ),
    ],
)
def test_untyped_args_are_refused(serve, cmd, args, error):
    status, reply = post(serve.port, cmd, args)
    assert status == 200 and reply["ok"] is False and reply["error"] == error
    assert untouched(serve.port)


def test_wait_takes_milliseconds_or_a_duration_string(serve):
    before = post(serve.port, "state")[1]["status"]["time"]
    assert post(serve.port, "wait", {"duration": 60000})[1]["ok"]
    assert post(serve.port, "wait", {"duration": "1m"})[1]["ok"]
    assert post(serve.port, "state")[1]["status"]["time"] != before


def test_shot_is_png_whatever_the_extension(serve, tmp_path):
    for name in ("noext", "a.jpg"):
        r = post(serve.port, "shot", {"path": str(tmp_path / name)})[1]
        assert r["ok"] and r["shot"] == str(tmp_path / name)
        assert (tmp_path / name).read_bytes()[:8] == b"\x89PNG\r\n\x1a\n"


def test_other_methods_and_paths_rejected_cleanly(serve):
    for method in ("GET", "PUT", "DELETE"):
        status, body = raw(serve.port, method, body=b"")
        assert status == 405 and body["ok"] is False and body["error"]
    status, body = raw(serve.port, "PUT", headers={"Origin": "http://evil.example", "Content-Type": "text/plain"})
    assert status == 405
    status, body = raw(serve.port, path="/other")
    assert status == 404 and body["ok"] is False and body["error"]
    assert untouched(serve.port)


@pytest.mark.parametrize(
    "body",
    [b"not json", b'{"cmd": 1}', b'{"cmd": "state", "args": [1]}', b"{}", b"[]", b'"state"', b"null"],
)
def test_bad_requests_are_400(serve, body):
    status, reply = raw(serve.port, body=body)
    assert status == 400 and reply["ok"] is False and reply["error"]
    assert untouched(serve.port)


def test_second_serve_is_refused(serve, start_serve):
    other = start_serve(HELLO, state="other-state", fresh=False, wait=False)
    assert other.wait_exit() == 2
    assert "a daemon is already running (pid" in other.errfile.read_text()


def test_port_in_use(cli, tmp_path):
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        s.listen()
        port = s.getsockname()[1]
        r = cli("serve", HELLO, "--port", port, "--state", tmp_path / "state", "--fixtures", tmp_path / "fx")
    assert r.returncode == 2
    assert str(port) in r.stderr and "cannot listen on" in r.stderr


def test_sigint_leaves_the_app_and_cleans_up(start_serve, make_app):
    app = make_app("function on_leave(c) c.state.left = 'yes' end")
    s = start_serve(app)
    s.proc.send_signal(signal.SIGINT)
    assert s.wait_exit() == 0
    assert not (s.root / "daemon.json").exists()
    assert json.loads((s.state / "state.json").read_text(encoding="utf-8"))["left"] == "yes"


def test_stop_exits_zero_and_cleans_up(serve):
    assert post(serve.port, "stop")[1]["ok"]
    assert serve.wait_exit() == 0
    assert not (serve.root / "daemon.json").exists()


def test_serve_exits_1_with_lua_errors(start_serve, make_app):
    app = make_app("function on_input(c, ev) error('boom') end")
    s = start_serve(app)
    r = post(s.port, "tap", {"x": 1, "y": 1})[1]
    assert r["errors"]
    assert post(s.port, "stop")[1]["ok"]
    assert s.wait_exit() == 1
    assert not (s.root / "daemon.json").exists()
