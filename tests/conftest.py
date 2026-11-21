import json
import threading
from collections.abc import Callable
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

import pytest

from inkemu import Emu


def pytest_configure(config):
    config.addinivalue_line("markers", "allow_errors: the test expects Lua errors in emu.errors")


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

        def log_message(self, *a):
            pass

    srv = HTTPServer(("127.0.0.1", 0), H)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    srv.hits, srv.base, srv.echo = hits, f"http://127.0.0.1:{srv.server_port}", False
    yield srv
    srv.shutdown()
    srv.server_close()
