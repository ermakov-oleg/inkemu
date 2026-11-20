import queue
import time
from pathlib import Path
from types import SimpleNamespace

import pytest

tk = pytest.importorskip("tkinter")

from inkemu import daemon, window  # noqa: E402

HELLO = Path(__file__).resolve().parent.parent / "examples" / "hello"


@pytest.fixture
def make_window(emu_factory):
    apps = []

    def make(app_dir=HELLO, control=None):
        try:
            root = tk.Tk()
        except tk.TclError as e:
            pytest.skip(f"no display: {e}")
        app = window.build_window(emu_factory(app_dir), control, root)
        apps.append(app)
        root.update()
        return app

    yield make
    for a in apps:
        try:
            a.root.destroy()
        except tk.TclError:
            pass


def click(app, x, y):
    ev = SimpleNamespace(x=x, y=y)
    app.on_press(ev)
    app.on_release(ev)
    app.root.update()


def key(app, name):
    app.on_key(SimpleNamespace(keysym=name))
    app.root.update()


def test_click_tap_me(make_window):
    app = make_window()
    item = next(t for t in app.emu.texts() if t["text"] == "Tap me")
    click(app, item["x"] + item["w"] // 2, item["y"] + item["h"] // 2)
    assert app.emu.has_text("Taps: 1")
    assert "net replay" in app.status.cget("text")


def test_keys_clock_pause_lock(make_window):
    app = make_window()
    t0 = app.emu.clock.ms
    key(app, "bracketright")
    assert app.emu.clock.ms - t0 >= 60_000
    key(app, "braceright")
    assert app.emu.clock.ms - t0 >= 3_660_000
    key(app, "p")
    assert app.emu.paused and "paused" in app.status.cget("text")
    key(app, "l")
    assert app.emu.locked
    key(app, "L")
    assert not app.emu.locked
    key(app, "r")
    assert app.emu.running


def test_screenshot_key(make_window):
    app = make_window()
    key(app, "s")
    assert list((app.emu.state_dir / "shots").glob("*.png"))


@pytest.mark.allow_errors
def test_lua_error_shows_in_status_not_raised(make_window, make_app):
    app = make_window(make_app("error('boom')"))
    click(app, 10, 10)
    key(app, "Return")
    assert "error:" in app.status.cget("text") and "boom" in app.status.cget("text")


ASKING = """
res = 'none'
function on_input(c, e)
  if e.phase == 'up' and e.key == 'ok' then
    %s
  end
  return true
end
function on_draw(c, g) g:clear(0); g:text(0, 0, 'res ' .. res) end
"""


def test_pending_dialog_is_deferred_and_blocks_input(make_window, make_app, monkeypatch):
    app = make_window(make_app(ASKING % (
        "c.dialog:confirm({title='Sure', text='Really?'}, function(c, ok) res = tostring(ok); c:invalidate() end)")))
    asked = []
    monkeypatch.setattr(window, "ask_yes_no", lambda parent, title, text: asked.append((title, text)) or True)
    app.on_key(SimpleNamespace(keysym="Return"))
    assert app.emu.pending and not asked  # opened from after_idle, not inside the Lua callback
    app.on_key(SimpleNamespace(keysym="Escape"))  # swallowed while the dialog is pending
    assert app.emu.pending
    app.root.update()
    assert asked == [("Sure", "Really?")]
    assert app.emu.pending is None and app.emu.has_text("res true")


def test_text_dialog_types_answer(make_window, make_app, monkeypatch):
    app = make_window(make_app(ASKING % (
        "c.input:text({title='Name', text=''}, function(c, s) res = s; c:invalidate() end)")))
    monkeypatch.setattr(window, "ask_string", lambda parent, title, text, initial: "Привет")
    key(app, "Return")
    app.root.update()
    assert app.emu.has_text("res Привет")


def test_text_dialog_cancel(make_window, make_app, monkeypatch):
    app = make_window(make_app(ASKING % (
        "c.input:text({title='Name', text=''}, function(c, s) res = s; c:invalidate() end)")))
    monkeypatch.setattr(window, "ask_string", lambda parent, title, text, initial: None)
    key(app, "Return")
    app.root.update()
    assert app.emu.pending is None and app.emu.has_text("res none")


def test_control_pump_runs_on_main_thread_and_freezes_clock(make_window):
    calls = []
    control = SimpleNamespace(stopped=False, pump=lambda: calls.append(1) or 0)
    app = make_window(control=control)
    for _ in range(2):
        app.root.update()
        app.root.after(80)
    app.root.update()
    assert calls and not app.emu.paused  # nothing ran: the clock keeps going

    d = daemon.Daemon(app.emu, port=0)
    try:
        app.control = d
        reply = queue.Queue(1)
        d.queue.put(("tap-text", {"text": "Tap me"}, reply))
        d.queue.put(("tap-text", {"text": "missing"}, queue.Queue(1)))
        for _ in range(3):
            app.root.update()
            app.root.after(80)
        app.root.update()
        assert reply.get_nowait()["ok"]
        assert app.emu.has_text("Taps: 1")
        assert app.emu.paused and app.remote_frozen
        assert "paused (remote)" in app.status.cget("text")
        d.queue.put(("resume", {}, queue.Queue(1)))
        for _ in range(2):
            app.root.update()
            app.root.after(80)
        assert not app.emu.paused and not app.remote_frozen
    finally:
        d.close()


def test_stop_command_closes_window_and_saves_state(make_window):
    app = make_window()
    d = daemon.Daemon(app.emu, port=0)
    try:
        app.control = d
        app.root.after(window.CONTROL_POLL_MS, app._poll_control)
        q = queue.Queue(1)
        d.queue.put(("stop", {}, q))
        for _ in range(4):
            try:
                app.root.update()
            except tk.TclError:
                break
            time.sleep(0.08)
        assert q.get_nowait()["ok"] and d.stopped
        assert app._closed and not app.emu.running
    finally:
        d.close()


GESTURES = """
res = 'none'
function on_input(c, e)
  if e.type == 'pointer' and e.phase == 'up' then res = tostring(e.gesture); c:invalidate() end
  return true
end
function on_draw(c, g) g:clear(0); g:text(0, 0, 'res ' .. res) end
"""


def real_wait(app, seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        app.root.update()
        time.sleep(0.01)


@pytest.mark.parametrize("paused", [False, True])
def test_real_hold_is_long_not_tap(make_window, make_app, paused):
    app = make_window(make_app(GESTURES))
    if paused:
        key(app, "p")
    ev = SimpleNamespace(x=100, y=100)
    app.on_press(ev)
    real_wait(app, 0.62)
    app.on_release(ev)
    assert app.emu.has_text("res long")


def test_close_leaves_and_persists_state(make_window, make_app, emu_factory):
    src = """
function on_load(c) c.state.n = (c.state.n or 0) + 1 end
function on_leave(c) c.state.left = true end
function on_draw(c, g) g:clear(0); g:text(0, 0, 'n ' .. c.state.n .. ' left ' .. tostring(c.state.left)) end
"""
    app = make_window(make_app(src))
    app.close()
    assert not app.emu.running
    assert emu_factory(app.emu.app_dir).has_text("n 2 left true")


def test_no_pointer_reaches_emulator_while_pending(make_window, make_app, monkeypatch):
    app = make_window(make_app("""
function on_input(c, e)
  if e.type == 'pointer' and e.phase == 'down' then
    c.dialog:confirm({title='T', text='x'}, function() end)
  end
  return true
end
function on_draw(c, g) g:clear(0) end
"""))
    app.on_press(SimpleNamespace(x=5, y=5))
    assert app.emu.pending and app._down
    calls = []
    monkeypatch.setattr(app.emu, "pointer", lambda *a: calls.append(a))
    app._fire_hold(5, 5)
    app.on_motion(SimpleNamespace(x=9, y=9))
    app.on_release(SimpleNamespace(x=9, y=9))
    assert calls == []
    monkeypatch.setattr(window, "ask_yes_no", lambda *a: True)
    app._down = True  # release lost to the dialog grab
    app.root.update()
    assert not app._down
    app._fire_hold(5, 5)
    assert calls == []
