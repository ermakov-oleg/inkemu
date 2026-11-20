"""Regenerate the README screenshots: `uv run python docs/make_images.py`."""
import tempfile
from pathlib import Path

from PIL import ImageOps

from inkemu import Emu

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "images"


def save(emu: Emu, name: str) -> None:
    img = ImageOps.expand(emu.shot().convert("L"), border=12, fill=255)
    ImageOps.expand(img, border=2, fill=96).save(OUT / name, optimize=True)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        emu = Emu(ROOT / "examples" / "hello", state_dir=Path(tmp) / "state", fixtures_dir=Path(tmp) / "fx",
                  start_time="2026-11-20T09:41:00")
        for _ in range(3):
            emu.tap_text("Tap me")
        save(emu, "hello.png")
        emu.lock()
        emu.wait("2s")
        save(emu, "lockscreen.png")
        assert emu.errors == [], emu.errors


if __name__ == "__main__":
    main()
