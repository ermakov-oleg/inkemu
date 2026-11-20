import json
import os
import re
from pathlib import Path


def state_root() -> Path:
    """`.emu` in the current directory, or `$INKEMU_STATE_ROOT`."""
    return Path(os.environ.get("INKEMU_STATE_ROOT", ".emu")).absolute()


def app_id_of(app_dir: str | Path, manifest: dict | None = None) -> str:
    """The manifest's app_id if it is Latin letters only, else the letters of the directory name."""
    # app_id comes from an untrusted manifest and names directories we may wipe
    if manifest is None:
        try:
            manifest = json.loads((Path(app_dir) / "manifest.json").read_text(encoding="utf-8"))
        except (OSError, ValueError):
            manifest = None
    app_id = manifest.get("app_id") if isinstance(manifest, dict) else None
    if isinstance(app_id, str) and re.fullmatch(r"[A-Za-z]+", app_id):
        return app_id
    return re.sub(r"[^A-Za-z]", "", Path(app_dir).resolve().name) or "app"
