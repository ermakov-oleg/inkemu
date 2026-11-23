import re

import pytest
from PIL import ImageChops

FONTS = "FONTS = {small = {name = 'small'}, medium = {name = 'medium'}, large = {name = 'large'}}\n"


@pytest.fixture
def draw(make_app, emu_factory):
    """An app whose on_draw runs `body` and keeps `g` in the global G."""

    def make(body: str, **manifest):
        return emu_factory(make_app(FONTS + f"function on_draw(c, g)\n  G = g\n  {body}\nend\n", manifest=manifest))

    return make


def ink_bbox(img):
    return ImageChops.invert(img.convert("L")).getbbox()


def test_text_center_align_box(draw):
    emu = draw("g:text(240, 40, 'Hi', {font=FONTS.large, align='center'})")
    item = emu.texts()[0]
    assert item["text"] == "Hi" and abs(item["x"] + item["w"] / 2 - 240) <= 1 and item["h"] == 30
    assert item["font"] == "large" and item["y"] == 40


def test_text_valign_and_right(draw):
    emu = draw(
        "g:text(100, 100, 'ab', {align='right', valign='middle'}); g:text(10, 100, 'x', {valign='bottom', font=FONTS.small})"
    )
    a, b = emu.texts()
    assert a["x"] + a["w"] == 100 and a["y"] == 100 - 12
    assert b["y"] == 100 - 20


def test_command_cap_drops_and_warns(draw):
    emu = draw("for i = 0, 1099 do g:rect(i % 400, i // 400, 1, 1, {fill=true}) end")
    assert emu.frame_commands == 1100 and emu.warnings and "1024" in emu.warnings[0]
    assert emu.raw_frame().convert("L").histogram()[0] == 1024


@pytest.mark.allow_errors
def test_dot_call_on_g_errors(draw):
    emu = draw("g.text(1, 2, 'x')")
    assert "colon" in emu.errors[0]


def test_cyrillic_text_measured_and_drawn(draw):
    emu = draw("g:text(10, 10, 'Привет •')")
    assert emu.lua_eval("return ctx.fonts.medium:measure('Привет •')") > 40, (
        "API 2.33.1 contract §6 says otherwise: g:measure is gone, font:measure replaces it (06/g_methods)"
    )
    assert ink_bbox(emu.raw_frame()) is not None
    assert emu.texts()[0]["text"] == "Привет •"


def test_render_rect_fill_is_black(draw):
    img = draw("g:clear(0); g:rect(10, 10, 20, 20, {fill=true})").raw_frame()
    assert img.mode == "1" and img.size == (480, 800)
    assert img.getpixel((15, 15)) == 0 and img.getpixel((5, 5)) == 255


def test_white_fill_color_zero(draw):
    img = draw("g:rect(0, 0, 50, 50, {fill=true}); g:rect(10, 10, 10, 10, {fill=true, color=0})").raw_frame()
    assert img.getpixel((15, 15)) == 255 and img.getpixel((5, 5)) == 0


def test_the_0_1_extras_of_g_are_gone(draw):
    emu = draw("")
    gone = "return G.round_rect == nil and G.pattern_rect == nil and G.fill_pattern == nil and G.measure_text == nil"
    assert emu.lua_eval(gone) is True, (
        "API 2.33.1 contract §6 says otherwise: firmware 26112001 has no g:pattern_rect (06/g_methods)"
    )


def test_shapes_do_not_crash(draw):
    emu = draw(
        """
      g:rect(5, 5, 100, 40, {radius=8, fill=true}); g:circle(50, 50, 10); g:arc(60, 60, 20, 180, 360, {line_width=3})
      g:polygon({10,10, 50,10, 30,40}, {fill=true}); g:polyline({1,1, 20,30, 40,5}, {width=2})
      g:line(0, 0, 100, 100, {line_width=2})
    """
    )
    assert ink_bbox(emu.raw_frame()) is not None, (
        "API 2.33.1 contract §6 says otherwise: g:round_rect is gone, g:rect{radius} replaces it (06/g_methods)"
    )


def test_text_box_wraps(draw):
    emu = draw("g:text_box(10, 10, 100, 200, 'one two three four five six seven eight', {font=FONTS.small})")
    item = emu.texts()[0]
    assert item["text"].startswith("one two") and item["w"] <= 100 and item["h"] > 20
    assert ink_bbox(emu.raw_frame().crop((10, 10 + 20, 110, 10 + 40))) is not None


def test_size_and_measure(draw):
    emu = draw("SIZE = {g:size()}; g:text(0, 0, 'Hi', {font=FONTS.large}); g:text(0, 100, 'Hi')")
    large, medium = emu.texts()
    assert emu.lua_eval("return SIZE[1] * 1000 + SIZE[2]") == 480800, (
        "API 2.33.1 contract §6 says otherwise: g:size is read inside the callback (06/canvas_inactive)"
    )
    assert emu.lua_eval("return ctx.fonts.large:measure('Hi')") == large["w"], (
        "API 2.33.1 contract §6 says otherwise: g:measure is gone, font:measure replaces it (06/g_methods)"
    )
    assert emu.lua_eval("return ctx.fonts.medium:measure('Hi')") == medium["w"], (
        "API 2.33.1 contract §6 says otherwise: g:measure is gone, font:measure replaces it (06/g_methods)"
    )
    assert emu.lua_eval("return select(2, G:size())") == "inactive", (
        "API 2.33.1 contract §6 says otherwise: g is valid only during its callback (06/canvas_inactive)"
    )


def test_resize(draw):
    emu = draw("W = g:size(); g:clear(0)", display={"orientation": "landscape"})
    assert emu.raw_frame().size == (800, 480)
    assert emu.lua_eval("return W") == 800, (
        "API 2.33.1 contract §6 says otherwise: g:size is read inside the callback (06/canvas_inactive)"
    )


def test_image_is_drawn_scaled(make_app, emu_factory):
    emu = emu_factory(
        make_app(
            "function on_load(c) layer = c.layers:create(4, 4); layer:draw(function(g) g:clear(1) end) end\n"
            "function on_draw(c, g) g:image(layer, 20, 20, {w=8, h=8}) end\n"
        )
    )
    img = emu.raw_frame()
    assert img.getpixel((22, 22)) == 0 and img.getpixel((30, 30)) == 255


def test_rtl_text_warns_and_is_skipped(draw):
    emu = draw("g:text(10, 10, 'שלום')")
    assert emu.texts() == [] and emu.warnings


def test_begin_frame_resets(draw):
    emu = draw("g:text(1, 1, msg or 'a')")
    emu.lua_exec("msg = 'b'; ctx:invalidate()")
    emu.wait(0)
    assert [t["text"] for t in emu.texts()] == ["b"]


def test_g_outside_on_draw_changes_nothing(draw):
    emu = draw("g:text(1, 1, 'a')")
    before = emu.raw_frame().tobytes()
    emu.lua_exec("G:text(0, 40, 'late'); G:rect(0, 100, 50, 50, {fill=true})")
    assert [t["text"] for t in emu.texts()] == ["a"] and emu.raw_frame().tobytes() == before


@pytest.mark.allow_errors
@pytest.mark.parametrize(
    "call, pattern",
    [
        ("g:rect(1, 2)", r"bad argument #3 to 'rect' \(number expected, got no value\)"),
        ("g:text(1, 2)", "text"),
        ("g:text('a', 2, 'x')", "'text'.*number expected, got string"),
    ],
)
def test_bad_g_args_raise_lua_error_with_location(draw, call, pattern):
    emu = draw(call)
    assert re.search(pattern, emu.errors[0]) and "index.lua:4:" in emu.errors[0], emu.errors


def test_frame_with_bad_command_does_not_crash(draw):
    emu = draw("g:text(1, 1, 'ok'); pcall(function() g:rect(1, 2) end); g:rect(0, 0, 5, 5, {fill=true})")
    assert emu.frame_commands == 2 and len(emu.texts()) == 1


MEASURE_MOVED = "API 2.33.1 contract §6 says otherwise: g:measure is gone, font:measure replaces it (06/g_methods)"
MEASURE_STRINGS = ["обновлено 13:50", "Сегодня · 20 ноября", "0123456789", "Ёжик…", "· 42 ·"]


def _ink_cols(img):
    w, h = img.size
    px = img.load()
    return [x for x in range(w) if any(px[x, y] == 0 for y in range(h))]


@pytest.mark.parametrize("font", ["small", "medium", "large"])
@pytest.mark.parametrize("s", MEASURE_STRINGS)
def test_measure_matches_drawn_advance(draw, font, s):
    # measure is an advance width, not an ink bbox, so compare the left edges of bars drawn before and after s.
    emu = draw(f"g:text(10, 10, '|{s}|', {{font=FONTS.{font}}}); g:text(10, 60, '|', {{font=FONTS.{font}}})")
    img = emu.raw_frame()
    bar = _ink_cols(img.crop((0, 60, img.width, 100)))
    cols = _ink_cols(img.crop((0, 10, img.width, 50)))
    bar_w = bar[-1] - bar[0]
    measured = emu.lua_eval(f"return ctx.fonts.{font}:measure('|{s}')")
    assert abs((cols[-1] - bar_w - cols[0]) - measured) <= 1, MEASURE_MOVED


@pytest.mark.parametrize("font", ["small", "medium", "large"])
@pytest.mark.parametrize("s", MEASURE_STRINGS)
def test_right_aligned_text_stays_left_of_x(draw, font, s):
    emu = draw(
        f"g:text(456, 10, '{s}', {{font=FONTS.{font}, align='right'}}); g:text(456 - c.fonts.{font}:measure('{s}'), 60, '{s}', {{font=FONTS.{font}}})"
    )
    img = emu.raw_frame()
    measured = emu.lua_eval(f"return ctx.fonts.{font}:measure('{s}')")
    for top in (10, 60):
        cols = _ink_cols(img.crop((0, top, img.width, top + 40)))
        assert cols[-1] < 456 and cols[0] >= 456 - measured - 1, MEASURE_MOVED
