import pytest

from inkemu import daemon

CONFIRM = ("taps = 0\nres = 'none'\n"
           "function on_load(c) c.dialog:confirm({title='Delete?', text='All notes go'}, "
           "function(c, ok) res = tostring(ok); c:invalidate() end) end\n"
           "function on_input(c, ev) if ev.phase == 'up' then taps = taps + 1 end c:invalidate() return true end\n"
           "function on_draw(c, g) g:clear(0); g:text(0, 0, 'taps ' .. taps .. ' res ' .. res) end\n")


def words(emu) -> list[str]:
    return [t["text"] for t in emu.texts()]


def test_pending_confirm_is_drawn_and_listed(make_app, emu_factory):
    emu = emu_factory(make_app(CONFIRM))
    plain = emu.canvas.render()
    shot = emu.shot()
    assert shot.tobytes() != plain.tobytes()
    assert {"Delete?", "All notes go", "Yes", "No"} <= set(words(emu))
    assert words(emu)[0] == "taps 0 res none"  # the app frame stays under the dialog
    assert emu.status()["pending"] == {"kind": "confirm", "title": "Delete?", "text": "All notes go"}


def test_window_can_ask_for_the_frame_without_the_dialog(make_app, emu_factory):
    emu = emu_factory(make_app(CONFIRM))
    assert emu.shot(dialog=False).tobytes() == emu.canvas.render().tobytes()


def test_input_does_not_reach_the_app_while_a_dialog_is_open(make_app, emu_factory):
    emu = emu_factory(make_app(CONFIRM))
    emu.tap(10, 10)
    emu.key("ok")
    emu.swipe("left")
    assert emu.has_text("taps 0 res none") and emu.pending is not None
    went = [ln for ln in emu.log if "went to the open confirm dialog" in ln]
    assert len(went) == 3, emu.log
    emu.answer(False)
    emu.tap(10, 10)
    assert emu.has_text("taps 1 res false")


@pytest.mark.parametrize("label, res", [("Yes", "true"), ("No", "false")])
def test_tap_text_on_dialog_button_answers(make_app, emu_factory, label, res):
    emu = emu_factory(make_app(CONFIRM))
    t0 = emu.clock.ms
    emu.tap_text(label)
    assert emu.pending is None and emu.has_text(f"taps 0 res {res}")
    assert emu.clock.ms - t0 == 350  # one input, not two


def test_text_dialog_plate_shows_field_and_buttons(make_app, emu_factory):
    emu = emu_factory(make_app(
        "got = 'none'\nfunction on_load(c) c.input:text({title='Name', text='Ann'}, "
        "function(c, s) got = s; c:invalidate() end) end\n"
        "function on_draw(c, g) g:clear(0); g:text(0, 0, 'got ' .. got) end\n"))
    assert {"Name", "Ann", "OK", "Cancel"} <= set(words(emu))
    emu.tap_text("OK")
    assert emu.has_text("got Ann") and emu.pending is None


def test_edit_dialog_cancel_button(make_app, emu_factory):
    emu = emu_factory(make_app(
        "ended = 'no'\nfunction on_load(c) c.input:edit({title='Note', text='', "
        "on_end=function(c, s, why) ended = why; c:invalidate() end}) end\n"
        "function on_draw(c, g) g:clear(0); g:text(0, 0, 'ended ' .. ended) end\n"))
    assert {"Note", "OK", "Cancel"} <= set(words(emu)) and "" not in words(emu)
    emu.tap_text("Cancel")
    assert emu.has_text("ended cancel")


def test_leave_dialog_is_drawn(make_app, emu_factory):
    emu = emu_factory(make_app("function on_input(c, ev) return false end\n"
                               "function on_draw(c, g) g:text(0, 0, 'app') end\n"))
    emu.key("back")
    assert {"Leave app?", "Yes", "No"} <= set(words(emu))
    emu.tap_text("No")
    assert emu.pending is None and emu.status()["running"]


def test_daemon_texts_show_the_dialog(make_app, emu_factory):
    emu = emu_factory(make_app(CONFIRM))
    r = daemon.execute(emu, "tap-text", {"text": "taps 0 res none"})
    assert r["ok"] and r["status"]["pending"]["kind"] == "confirm"
    assert "Yes" in [t["text"] for t in r["texts"]]
    assert any("went to the open confirm dialog" in ln for ln in r["log_tail"])
    assert daemon.execute(emu, "tap-text", {"text": "Yes"})["status"]["pending"] is None
