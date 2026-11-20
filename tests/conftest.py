import json
from pathlib import Path
from typing import Callable

import pytest


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
