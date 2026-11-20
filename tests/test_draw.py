import pytest
from lupa import LuaError

from inkemu.draw import Canvas
from inkemu.fonts import Fonts
from inkemu.lua_env import make_object, new_runtime


@pytest.fixture
def env():
    L = new_runtime()
    fonts = Fonts()
    canvas = Canvas(L, fonts)
    warnings = []
    canvas.on_warning = warnings.append
    L.globals().G = canvas.g
    L.globals().FONTS = L.table_from({n: L.table_from({"name": n}) for n in ("small", "medium", "large")})
    return L, fonts, canvas, warnings


def run(env, code):
    L, _, canvas, _ = env
    canvas.begin_frame()
    L.execute(code)
    return canvas.end_frame()


def test_text_center_align_box(env):
    run(env, "G:text(240, 40, 'Hi', {font=FONTS.large, align='center'})")
    item = env[2].texts()[0]
    assert item["text"] == "Hi" and abs(item["x"] + item["w"] / 2 - 240) <= 1 and item["h"] == 30
    assert item["font"] == "large" and item["y"] == 40


def test_text_valign_and_right(env):
    run(env, "G:text(100, 100, 'ab', {align='right', valign='middle'}); G:text(10, 100, 'x', {valign='bottom', font=FONTS.small})")
    a, b = env[2].texts()
    assert a["x"] + a["w"] == 100 and a["y"] == 100 - 12
    assert b["y"] == 100 - 20


def test_command_cap_drops_and_warns(env):
    _, _, canvas, warnings = env
    n = run(env, "for i = 1, 1100 do G:line(0, 0, i % 400, 5) end")
    assert n == 1100 and warnings and "1024" in warnings[0]
    assert len(canvas._cmds) == 1024


def test_dot_call_on_g_errors(env):
    with pytest.raises(LuaError, match="colon"):
        run(env, "G.text(1, 2, 'x')")


def test_cyrillic_text_measured_and_drawn(env):
    _, fonts, canvas, _ = env
    assert fonts.measure("medium", "Привет •") > 40
    run(env, "G:text(10, 10, 'Привет •')")
    assert canvas.render().getbbox() is not None
    assert canvas.texts()[0]["text"] == "Привет •"


def test_render_rect_fill_is_black(env):
    run(env, "G:clear(0); G:rect(10, 10, 20, 20, {fill=true})")
    img = env[2].render()
    assert img.mode == "1" and img.size == (480, 800)
    assert img.getpixel((15, 15)) == 0 and img.getpixel((5, 5)) == 255


def test_white_fill_color_zero(env):
    run(env, "G:rect(0, 0, 50, 50, {fill=true}); G:rect(10, 10, 10, 10, {fill=true, color=0})")
    img = env[2].render()
    assert img.getpixel((15, 15)) == 255 and img.getpixel((5, 5)) == 0


def test_pattern_rect_is_checker(env):
    run(env, "G:pattern_rect(0, 0, 10, 10)")
    img = env[2].render()
    px = [img.getpixel((x, y)) for y in range(10) for x in range(10)]
    assert px.count(0) == 50 and img.getpixel((0, 0)) != img.getpixel((1, 0))


def test_shapes_do_not_crash(env):
    run(env, """
      G:round_rect(5, 5, 100, 40, 8, {fill=true}); G:circle(50, 50, 10); G:arc(60, 60, 20, 180, 360, {line_width=3})
      G:polygon({10,10, 50,10, 30,40}, {fill=true}); G:polyline({1,1, 20,30, 40,5}, {width=2})
      G:line(0, 0, 100, 100, {line_width=2})
    """)
    assert env[2].render().getbbox() is not None


def test_text_box_wraps(env):
    run(env, "G:text_box(10, 10, 100, 200, 'one two three four five six seven eight', {font=FONTS.small})")
    item = env[2].texts()[0]
    assert item["text"].startswith("one two") and item["w"] <= 100 and item["h"] > 20
    img = env[2].render()
    assert img.crop((10, 10 + 20, 110, 10 + 40)).getbbox() is not None


def test_size_and_measure(env):
    L = env[0]
    L.globals().R = None
    assert L.eval("(function() local w, h = G:size(); return w * 1000 + h end)()") == 480800
    assert L.eval("G:measure('Hi', {font=FONTS.large})") == env[1].measure("large", "Hi")
    assert L.eval("G:measure_text('Hi')") == env[1].measure("medium", "Hi")


def test_resize(env):
    env[2].resize(800, 480)
    run(env, "G:clear(0)")
    assert env[2].render().size == (800, 480)
    assert env[0].eval("(function() local w, h = G:size(); return w end)()") == 800


def test_image_hook(env):
    from PIL import Image
    _, _, canvas, _ = env
    canvas.image_source = lambda obj: Image.new("1", (4, 4), 0)
    run(env, "G:image({}, 20, 20, {w=8, h=8})")
    img = canvas.render()
    assert img.getpixel((22, 22)) == 0 and img.getpixel((30, 30)) == 255


def test_rtl_text_warns_and_is_skipped(env):
    run(env, "G:text(10, 10, 'שלום')")
    assert env[2].texts() == [] and env[3]


def test_begin_frame_resets(env):
    run(env, "G:text(1, 1, 'a')")
    run(env, "G:text(1, 1, 'b')")
    assert [t["text"] for t in env[2].texts()] == ["b"]


def test_sandbox():
    L = new_runtime()
    for expr in ("io", "os", "dofile", "loadfile", "debug", "collectgarbage", "package.loadlib", "package.cpath", "python"):
        assert L.eval(expr) is None, expr
    assert L.eval("load('return 1 + 1')()") == 2
    with pytest.raises(LuaError):
        L.execute("load(function() return nil end)")
    assert L.eval("select('#', load(string.dump(function() end)))") == 2  # nil, message: binary chunks refused


def test_make_object_colon_check():
    L = new_runtime()
    obj = make_object(L, "clock", {"millis": lambda: 42}, {"tz": "UTC"})
    L.globals().obj = obj
    assert L.eval("obj:millis()") == 42 and L.eval("obj.tz") == "UTC"
    with pytest.raises(LuaError) as e:
        L.execute("obj.millis()")
    assert "bad argument #2 to 'millis' (call ctx.clock:millis with a colon)" in str(e.value)
