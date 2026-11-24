import json
import os
from pathlib import Path

import pytest

FIXTURE_FONTS = Path(__file__).resolve().parent / "fixtures" / "fonts"
# a captured device font is proprietary and never in the repository; read at import, before the autouse fixture
DEVICE_FONTS = os.environ.get("INKEMU_DEVICE_FONTS", "")

METRICS = (
    "return ('%d %d %d %d %d %d %d'):format(ctx.fonts.small.line_height, ctx.fonts.small.ascent, "
    "ctx.fonts.medium.line_height, ctx.fonts.medium.ascent, ctx.fonts.large.line_height, ctx.fonts.large.ascent, "
    "ctx.fonts.medium:measure('AB W'))"
)
# the synthetic fixture fonts: line heights 20/24/30, ascents 14/16/20, medium advances A 8, B 5, space 5, W 11
FIXTURE_METRICS = "20 14 24 16 30 20 29"


@pytest.fixture
def app(make_app):
    return make_app("function on_draw(c, g) g:text(10, 20, 'A') end\n")


def a_glyph_at(emu, x, y):
    """The fixture's 'A': a 5×7 box with a bar on its fourth row, one pixel right of the pen."""
    img = emu.raw_frame()
    ink = {(px, py) for py in range(y, y + 7) for px in range(x, x + 5) if img.getpixel((px, py)) == 0}
    rows = [(x, y), (x + 4, y), (x + 2, y + 3), (x, y + 6), (x + 4, y + 6)]
    return all(p in ink for p in rows) and (x + 2, y + 1) not in ink


def test_fonts_dir_draws_and_measures_with_the_bdf_files(app, emu_factory):
    emu = emu_factory(app, fonts_dir=FIXTURE_FONTS)
    assert emu.lua_eval(METRICS) == FIXTURE_METRICS
    # baseline = 20 + ascent 16; the glyph's 7 rows end on it
    assert a_glyph_at(emu, 11, 29)
    assert emu.texts()[0]["w"] == 8


def test_inkemu_fonts_env_switches_the_fonts(app, emu_factory, monkeypatch):
    monkeypatch.setenv("INKEMU_FONTS", str(FIXTURE_FONTS))
    emu = emu_factory(app)
    assert emu.lua_eval(METRICS) == FIXTURE_METRICS
    assert a_glyph_at(emu, 11, 29)


def test_without_fonts_the_bundled_font_draws(app, emu_factory, monkeypatch):
    monkeypatch.setenv("INKEMU_FONTS", "")
    emu = emu_factory(app)
    assert emu.lua_eval(METRICS) != FIXTURE_METRICS
    assert emu.lua_eval("return ctx.fonts.medium.line_height") == 24
    assert emu.lua_eval("return ctx.fonts.medium.ascent") != 16


def test_fonts_dir_beats_the_env(app, emu_factory, monkeypatch, tmp_path):
    monkeypatch.setenv("INKEMU_FONTS", str(tmp_path / "missing"))
    emu = emu_factory(app, fonts_dir=FIXTURE_FONTS)
    assert emu.lua_eval(METRICS) == FIXTURE_METRICS


def test_a_fonts_dir_without_the_three_files_is_refused(app, emu_factory, tmp_path):
    (tmp_path / "half").mkdir()
    (tmp_path / "half" / "small.bdf").write_bytes((FIXTURE_FONTS / "small.bdf").read_bytes())
    with pytest.raises(ValueError, match=r"fonts: cannot read .*medium\.bdf"):
        emu_factory(app, fonts_dir=tmp_path / "half")
    bad = tmp_path / "bad"
    bad.mkdir()
    for name in ("small", "medium", "large"):
        (bad / f"{name}.bdf").write_bytes((FIXTURE_FONTS / f"{name}.bdf").read_bytes())
    (bad / "large.bdf").write_text("not a font")
    with pytest.raises(ValueError, match=r"large\.bdf: not a BDF font"):
        emu_factory(app, fonts_dir=bad)


@pytest.mark.skipif(not DEVICE_FONTS, reason="INKEMU_DEVICE_FONTS is not set")
def test_device_font_widths_match_the_reader(make_app, emu_factory):
    """INKEMU_DEVICE_FONTS: a folder with the device's BDF files and device-fonts.json (widths measured on a reader)."""
    fonts = Path(DEVICE_FONTS)
    expected = json.loads((fonts / "device-fonts.json").read_text(encoding="utf-8"))["tiers"]
    emu = emu_factory(make_app("function on_draw(c, g) end\n"), fonts_dir=fonts)
    for tier, want in expected.items():
        got = emu.lua_eval(f"local f = ctx.fonts.{tier} return {{f.line_height, f.ascent}}")
        assert got == [want["line_height"], want["ascent"]], tier
        for s, width in want["samples"].items():
            lua_s = json.dumps(s, ensure_ascii=False)  # Lua 5.5 has no \uXXXX escape
            assert emu.lua_eval(f"return ctx.fonts.{tier}:measure({lua_s})") == width, (tier, s)
