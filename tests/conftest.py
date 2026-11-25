import contextlib
import json
import os
import queue
import signal
import subprocess
import sys
import threading
import time
from collections.abc import Callable
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

import pytest

from inkemu import Emu


def pytest_configure(config):
    config.addinivalue_line("markers", "allow_errors: the test expects Lua errors in emu.errors")


@pytest.fixture(autouse=True)
def _bundled_fonts(monkeypatch):
    """A shell's INKEMU_FONTS must not change the frames the suite pins."""
    monkeypatch.delenv("INKEMU_FONTS", raising=False)


@pytest.fixture
def make_app(tmp_path) -> Callable[..., Path]:
    def make(index_lua: str, *, manifest: dict | None = None, files: dict[str, str] | None = None) -> Path:
        app = tmp_path / "app"
        app.mkdir(exist_ok=True)
        m = {"app_id": "testapp", "display_name": "Test", "entry": "index.lua", "api": "2.2", "permissions": []}
        m.update(manifest or {})
        (app / "manifest.json").write_text(json.dumps(m), encoding="utf-8")
        (app / "index.lua").write_text(index_lua, encoding="utf-8")
        for name, text in (files or {}).items():
            p = app / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text, encoding="utf-8")
        return app

    return make


@pytest.fixture
def emu_factory(tmp_path, request) -> Callable[..., Emu]:
    made: list[Emu] = []

    def factory(app_dir: Path, **kw) -> Emu:
        kw.setdefault("state_dir", tmp_path / "state")
        kw.setdefault("fixtures_dir", tmp_path / "fixtures")
        kw.setdefault("seed_files", True)
        kw.setdefault("start_time", "2026-11-20T09:00:00")
        emu = Emu(app_dir, **kw)
        made.append(emu)
        return emu

    yield factory
    if request.node.get_closest_marker("allow_errors") is None:
        for emu in made:
            assert emu.errors == [], "unexpected Lua errors:\n" + "\n".join(emu.errors)


@pytest.fixture
def server():
    hits = []
    calls = []  # (method, body) of the requests other than GET

    class H(BaseHTTPRequestHandler):
        def do_GET(self):
            hits.append((self.path, dict(self.headers)))
            if self.path.startswith("/short"):  # promises more than it sends
                self.send_response(200)
                self.send_header("Content-Length", "100")
                self.end_headers()
                self.wfile.write(b"ab")
                return
            body = "привет".encode() if self.path.endswith(".ics") else b"\x00\xff\xfeok"
            if srv.echo or self.path.startswith("/echo"):
                body = self.path.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Set-Cookie", "sid=abc")
            self.end_headers()
            self.wfile.write(body)

        def other(self):
            body = self.rfile.read(int(self.headers.get("Content-Length") or 0))
            calls.append((self.command, body))
            self.send_response(200)
            self.send_header("X-Method", self.command)
            self.send_header("Content-Length", "4")
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(b"done")

        do_PUT = do_HEAD = do_PROPFIND = other

        def do_REPORT(self):
            body = self.rfile.read(int(self.headers.get("Content-Length") or 0))
            calls.append((self.command, body))
            self.send_response(207)
            self.send_header("Content-Length", str(len(body) + 3))
            self.end_headers()
            self.wfile.write(b"re:" + body)

        def log_message(self, *a):
            pass

    srv = HTTPServer(("127.0.0.1", 0), H)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    srv.hits, srv.base, srv.echo = hits, f"http://127.0.0.1:{srv.server_port}", False
    srv.calls = calls
    yield srv
    srv.shutdown()
    srv.server_close()


HELLO = Path(__file__).resolve().parent.parent / "examples" / "hello"
MCP_BIN = Path(sys.executable).parent / "inkemu-mcp"


@pytest.fixture
def root(tmp_path) -> Path:
    """INKEMU_STATE_ROOT of the processes a test starts."""
    return tmp_path / "root"


@pytest.fixture
def cli(tmp_path, root) -> Callable[..., subprocess.CompletedProcess]:
    """`inkemu ARGS` as a subprocess; the state root is `root`, the cwd is `tmp_path`."""

    def run(*args, cwd=None, timeout=60, env=None) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, "-m", "inkemu", *map(str, args)],
            env={**os.environ, "INKEMU_STATE_ROOT": str(root), **(env or {})},
            cwd=cwd or tmp_path,
            capture_output=True,
            text=True,
            timeout=timeout,
        )

    return run


class Serve:
    """A running `inkemu serve` subprocess."""

    def __init__(self, proc: subprocess.Popen, root: Path, state: Path, errfile: Path):
        self.proc, self.root, self.state, self.errfile = proc, root, state, errfile

    @property
    def info(self) -> dict:
        return json.loads((self.root / "daemon.json").read_text(encoding="utf-8"))

    @property
    def port(self) -> int:
        return self.info["port"]

    def wait_exit(self, timeout: float = 20) -> int:
        return self.proc.wait(timeout)


def wait_for_daemon_json(root: Path, proc: subprocess.Popen | None = None, timeout: float = 20) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            info = json.loads((root / "daemon.json").read_text(encoding="utf-8"))
            if info["port"]:
                return info
        except (OSError, ValueError, KeyError):
            pass
        if proc is not None and proc.poll() is not None:
            raise AssertionError(f"serve exited with {proc.returncode} before it was ready")
        time.sleep(0.05)
    raise AssertionError("daemon.json did not appear")


def _kill_daemon_of(root: Path) -> None:
    with contextlib.suppress(OSError, ValueError, KeyError):
        os.kill(int(json.loads((root / "daemon.json").read_text(encoding="utf-8"))["pid"]), signal.SIGKILL)


@pytest.fixture
def start_serve(tmp_path, root) -> Callable[..., Serve]:
    """`start_serve(app, *extra_args)`: `inkemu serve APP --port 0` with a fresh state by default; killed at teardown."""
    started: list[subprocess.Popen] = []

    def start(app: Path = HELLO, *extra, wait: bool = True, state: str = "state", fresh: bool = True) -> Serve:
        state_dir = tmp_path / state
        errfile = tmp_path / ("serve.err" if state == "state" else f"{state}.err")
        args = ["serve", str(app), "--port", "0", "--state", str(state_dir), "--fixtures", str(tmp_path / "fx")]
        with errfile.open("w") as err:
            proc = subprocess.Popen(
                [sys.executable, "-m", "inkemu", *args, *(["--fresh"] if fresh else []), *map(str, extra)],
                env={**os.environ, "INKEMU_STATE_ROOT": str(root)},
                cwd=tmp_path,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=err,
            )
        started.append(proc)
        if wait:
            wait_for_daemon_json(root, proc)
        return Serve(proc, root, state_dir, errfile)

    yield start
    for proc in started:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
    _kill_daemon_of(root)


@pytest.fixture
def serve(start_serve) -> Serve:
    return start_serve(HELLO)


class McpClient:
    """Synchronous newline-delimited JSON-RPC client of an `inkemu-mcp` process."""

    def __init__(self, env: dict, cwd: Path):
        self.proc = subprocess.Popen(
            [str(MCP_BIN)],
            env=env,
            cwd=cwd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        self.lines: queue.Queue = queue.Queue()
        threading.Thread(target=self._pump, daemon=True).start()
        self._id = 0
        self.request(
            "initialize",
            {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "tests", "version": "0"}},
        )
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def _pump(self) -> None:
        for line in self.proc.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def _send(self, msg: dict) -> None:
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()

    def request(self, method: str, params: dict | None = None, timeout: float = 60) -> dict:
        self._id += 1
        self._send({"jsonrpc": "2.0", "id": self._id, "method": method, "params": params or {}})
        while True:
            line = self.lines.get(timeout=timeout)
            assert line is not None, "inkemu-mcp closed stdout"
            msg = json.loads(line)
            if msg.get("id") == self._id:
                assert "error" not in msg, msg
                return msg["result"]

    def tools(self) -> list[dict]:
        return self.request("tools/list")["tools"]

    def call(self, name: str, **arguments) -> dict:
        """The raw tools/call result (content, isError)."""
        return self.request("tools/call", {"name": name, "arguments": arguments})

    def payload(self, name: str, **arguments) -> dict:
        return json.loads(self.call(name, **arguments)["content"][0]["text"])

    def close(self) -> None:
        try:
            self.proc.stdin.close()
            self.proc.wait(5)
        except (OSError, subprocess.TimeoutExpired):
            self.proc.kill()
            self.proc.wait()


@pytest.fixture
def mcp(tmp_path, root) -> Callable[..., McpClient]:
    """`mcp()` starts `inkemu-mcp` with TMPDIR=<tmp>/tmp and the shared state root; closed at teardown."""
    clients: list[McpClient] = []
    (tmp_path / "tmp").mkdir()

    def start() -> McpClient:
        env = {**os.environ, "INKEMU_STATE_ROOT": str(root), "TMPDIR": str(tmp_path / "tmp")}
        client = McpClient(env, tmp_path)
        clients.append(client)
        return client

    yield start
    for c in clients:
        c.close()
    _kill_daemon_of(root)


@pytest.fixture
def foreign_server():
    """Something that answers HTTP but is not an inkemu daemon."""

    class H(BaseHTTPRequestHandler):
        def do_POST(self):
            self.rfile.read(int(self.headers["Content-Length"]))
            self.send_response(502)
            self.send_header("Content-Type", "text/html")
            self.end_headers()
            self.wfile.write(b"<html>bad gateway</html>")

        def log_message(self, *a):
            pass

    srv = HTTPServer(("127.0.0.1", 0), H)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    yield srv.server_port
    srv.shutdown()
    srv.server_close()
