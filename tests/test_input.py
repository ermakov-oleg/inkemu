import pytest

LOGGER = """
function on_load(ctx) ctx.state.ev = {} end
function on_input(ctx, e)
  table.insert(ctx.state.ev, e.type .. ':' .. e.phase .. ':' .. (e.gesture or '') .. ':' .. (e.key or ''))
  if e.phase == 'up' then ctx.state.last = e end
  ctx:invalidate()
end
function on_draw(ctx, g) g:clear() g:text(10, 10, 'Taps: ' .. #ctx.state.ev) end
"""


def events(emu):
    out = emu.lua_eval("local t = {} for i, v in ipairs(ctx.state.ev) do t[i] = v end return table.concat(t, '|')")
    return out.split("|") if out else []


def last(emu, field):
    return emu.lua_eval("return ctx.state.last." + field)


@pytest.fixture
def emu(make_app, emu_factory):
    return emu_factory(make_app(LOGGER))


def test_tap_sequence(emu):
    emu.tap(10, 10)
    assert events(emu) == ["pointer:down::", "pointer:up:tap:"]


def test_swipe_left(emu):
    emu.swipe("left")
    ev = events(emu)
    assert ev[-1] == "pointer:up:swipe_left:" and any(e.startswith("pointer:move") for e in ev)


@pytest.mark.parametrize("direction", ["left", "right", "up", "down"])
def test_swipe_directions(emu, direction):
    emu.swipe(direction, distance=100)
    assert events(emu)[-1] == f"pointer:up:swipe_{direction}:"


def test_swipe_bad_direction(emu):
    with pytest.raises(ValueError):
        emu.swipe("diagonal")


def test_hold_then_long(emu):
    emu.hold(5, 5, ms=800)
    assert events(emu)[-2:] == ["pointer:hold::", "pointer:up:long:"]


def test_hold_event_arrives_at_600ms(make_app, emu_factory):
    src = LOGGER.replace("e.type ..", "e.time_ms .. ':' .. e.type ..")
    emu = emu_factory(make_app(src))
    emu.hold(5, 5, ms=700)
    ev = events(emu)
    assert ev[-2].endswith(":pointer:hold::")
    assert int(ev[-2].split(":")[0]) - int(ev[0].split(":")[0]) == 600


def test_two_taps_are_not_double(emu):
    emu.tap(5, 5)
    emu.tap(5, 5)
    assert "pointer:up:double_tap:" not in events(emu)


def test_double_tap(emu):
    emu.double_tap(5, 5)
    assert events(emu)[-1] == "pointer:up:double_tap:"


def test_key_down_up(emu):
    emu.key("up")
    assert events(emu)[-2:] == ["key:down::up", "key:up::up"]


def test_every_input_advances_350ms(emu):
    t0 = emu.clock.ms
    emu.tap(1, 1)
    emu.swipe("up")
    emu.hold(1, 1, ms=700)
    emu.double_tap(1, 1)
    emu.key("ok")
    assert emu.clock.ms - t0 >= 5 * 350


def test_pointer_classifies_on_up_with_virtual_time(emu):
    emu.pointer("down", 100, 100)
    emu.wait(700)
    emu.pointer("up", 105, 100)
    assert events(emu)[-1] == "pointer:up:long:"


def test_pointer_small_move_is_tap_and_down_pos_kept(emu):
    emu.pointer("down", 100, 100)
    emu.pointer("move", 110, 100)
    emu.pointer("up", 110, 100)
    assert events(emu)[-1] == "pointer:up:tap:"
    assert (last(emu, "down_x"), last(emu, "x")) == (100, 110)


def test_pointer_swipe_dominant_axis(emu):
    emu.pointer("down", 100, 100)
    emu.pointer("up", 190, 130)
    assert events(emu)[-1] == "pointer:up:swipe_right:"


def test_pointer_up_without_down_is_ignored(emu):
    emu.pointer("up", 1, 1)
    assert events(emu) == []


def test_pointer_unknown_phase(emu):
    with pytest.raises(ValueError):
        emu.pointer("wiggle", 1, 1)


def test_drag_classified_by_thresholds(emu):
    emu.drag([(50, 50), (60, 50), (200, 52)], duration_ms=300)
    assert events(emu)[-1] == "pointer:up:swipe_right:"
    emu.drag([(50, 50), (52, 51)], duration_ms=100)
    assert events(emu)[-1] == "pointer:up:tap:"


def test_drag_needs_two_points(emu):
    with pytest.raises(ValueError):
        emu.drag([(1, 1)])


def test_tap_text_missing_lists_texts(emu):
    with pytest.raises(LookupError, match="Taps: 0"):
        emu.tap_text("Nope")


PICK = """
function on_load(ctx) ctx.state.ev = {} end
function on_input(ctx, e) if e.phase == 'up' then ctx.state.y = e.y end end
function on_draw(ctx, g)
  g:clear()
  g:text(10, 10, 'Go')
  g:text(10, 600, 'Go')
  g:text(10, 300, 'Go on')
  g:text(10, 700, 'Go on again')
end
"""


def test_tap_text_picks_last_drawn(make_app, emu_factory):
    emu = emu_factory(make_app(PICK))
    emu.tap_text("Go")
    y = emu.lua_eval("return ctx.state.y")
    assert 590 < y < 640


def test_tap_text_exact_beats_substring(make_app, emu_factory):
    emu = emu_factory(make_app(PICK))
    emu.tap_text("Go on")
    assert emu.lua_eval("return ctx.state.y") < 340


def test_tap_text_substring_fallback(make_app, emu_factory):
    emu = emu_factory(make_app(PICK))
    emu.tap_text("again")
    assert emu.lua_eval("return ctx.state.y") > 690
