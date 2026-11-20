import json

import pytest

DRAW_MSG = "msg = 'none'\nfunction on_draw(c, g) g:clear(0); g:text(0, 0, 'msg ' .. tostring(msg)) end\n"


@pytest.mark.allow_errors
def test_dot_call_on_ctx_is_error(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c) ctx.clock.now() end"))
    assert "colon" in emu.errors[0] and "index.lua:1:" in emu.errors[0]
    assert emu.status()["running"] is False


@pytest.mark.allow_errors
def test_dot_call_on_ctx_level_method_is_error(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c) ctx.invalidate() end"))
    assert "ctx:invalidate" in emu.errors[0]


@pytest.mark.allow_errors
def test_lua_error_is_captured(make_app, emu_factory):
    app = make_app("local function boom() error('kaput') end\n"
                   "function on_draw(c, g) g:clear(0); g:text(0, 0, 'before'); boom() end\n")
    emu = emu_factory(app)
    assert "index.lua:1: kaput" in emu.errors[0] and "stack traceback" in emu.errors[0]
    assert "index.lua:2: in function 'on_draw'" in emu.errors[0] and "<python>" not in emu.errors[0]
    assert emu.has_text("App error")
    assert emu.shot().tobytes() != emu.canvas.render().tobytes()
    emu.tap(10, 10)
    emu.key("back")
    emu.wait("1s")
    assert emu.status()["running"] is False and len(emu.errors) == 1


@pytest.mark.allow_errors
def test_timer_error_stops_app_and_cancels_timers(make_app, emu_factory):
    app = make_app(DRAW_MSG + "function on_load(c)\n  c:after(100, function() error('late') end)\n"
                   "  c:after(500, function() msg = 'second' end)\nend\n")
    emu = emu_factory(app)
    emu.wait(1000)
    assert len(emu.errors) == 1 and "index.lua:4: late" in emu.errors[0]
    assert emu.has_text("App error") and not emu.has_text("msg second")


@pytest.mark.allow_errors
def test_python_side_error_has_lua_position(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c)\n  c:after('soon', function() end)\nend"))
    assert "index.lua:2:" in emu.errors[0] and "after" in emu.errors[0]


def test_app_pcall_sees_string_error(make_app, emu_factory):
    app = make_app(DRAW_MSG + "local ok, e = pcall(function() ctx:after('x') end)\nmsg = type(e)\n")
    assert emu_factory(app).has_text("msg string")


@pytest.mark.allow_errors
def test_syntax_error_reported(make_app, emu_factory):
    emu = emu_factory(make_app("function ("))
    assert emu.errors and "index.lua:1:" in emu.errors[0]
    assert emu.has_text("App error") and emu.status()["running"] is False


@pytest.mark.allow_errors
def test_reload_after_fixing_code(make_app, emu_factory):
    app = make_app("function (")
    emu = emu_factory(app)
    (app / "index.lua").write_text(DRAW_MSG + "msg = 'fixed'", encoding="utf-8")
    emu.reload()
    assert emu.status()["running"] is True and emu.has_text("msg fixed") and not emu.has_text("App error")


@pytest.mark.allow_errors
def test_manifest_api_above_firmware_refused(make_app, emu_factory):
    emu = emu_factory(make_app("ctx:log('ran')", manifest={"api": "2.30"}))
    assert "needs firmware with API 2.30" in emu.errors[0]
    assert emu.status()["running"] is False and not any("ran" in line for line in emu.log)


def test_manifest_api_equal_to_firmware_runs(make_app, emu_factory):
    emu = emu_factory(make_app("ctx:log('ran')", manifest={"api": "2.26"}))
    assert emu.status()["running"] is True and any("ran" in line for line in emu.log)


@pytest.mark.allow_errors
@pytest.mark.parametrize("manifest, needle", [
    ({"app_id": "hello2"}, "app_id"),
    ({"permissions": ["net", "wifi"]}, "wifi"),
])
def test_manifest_strictness(make_app, emu_factory, manifest, needle):
    emu = emu_factory(make_app("", manifest=manifest))
    assert needle in emu.errors[0] and emu.status()["running"] is False


@pytest.mark.allow_errors
@pytest.mark.parametrize("bad_id", ["absolute", "../../victim", "../victim"])
def test_bad_app_id_never_touches_disk_outside_state_root(make_app, emu_factory, tmp_path, monkeypatch, bad_id):
    victim = tmp_path / "victim"
    for sub in ("files", "shots"):
        (victim / sub).mkdir(parents=True)
        (victim / sub / "keep.txt").write_text("x")
    (victim / "state.json").write_text("{}")
    root = tmp_path / "root" / "emu"
    monkeypatch.setenv("INKEMU_STATE_ROOT", str(root))
    app_id = str(victim) if bad_id == "absolute" else bad_id
    emu = emu_factory(make_app("", manifest={"app_id": app_id}), state_dir=None, fresh=True)
    assert "app_id" in emu.errors[0] and emu.status()["running"] is False
    assert (victim / "files" / "keep.txt").exists() and (victim / "shots" / "keep.txt").exists()
    assert (victim / "state.json").exists()
    assert emu.state_dir.resolve().is_relative_to(root.resolve())


def test_derived_state_dir_uses_app_id(make_app, emu_factory, tmp_path, monkeypatch):
    monkeypatch.setenv("INKEMU_STATE_ROOT", str(tmp_path / "root"))
    emu = emu_factory(make_app(""), state_dir=None)
    assert emu.state_dir == tmp_path / "root" / "testapp"


def test_landscape_manifest_canvas(make_app, emu_factory):
    emu = emu_factory(make_app(DRAW_MSG, manifest={"display": {"orientation": "landscape"}}))
    assert emu.shot().size == (800, 480)
    assert emu.status()["screen"] == [800, 480]


def test_set_tick_calls_on_tick(make_app, emu_factory):
    app = make_app(DRAW_MSG + "n = 0\nfunction on_load(c) c:set_tick(100) end\n"
                   "function on_tick(c) n = n + 1; msg = n; c:invalidate() end\n")
    emu = emu_factory(app)
    emu.wait(1000)
    assert emu.texts()[0]["text"] == "msg 10"
    emu.wait(500)
    assert emu.texts()[0]["text"] == "msg 15"


def test_set_tick_zero_stops(make_app, emu_factory):
    app = make_app(DRAW_MSG + "n = 0\nfunction on_load(c) c:set_tick(100) end\n"
                   "function on_tick(c) n = n + 1; msg = n; if n == 3 then c:set_tick(0) end; c:invalidate() end\n")
    emu = emu_factory(app)
    emu.wait(1000)
    assert emu.texts()[0]["text"] == "msg 3"


@pytest.mark.allow_errors
@pytest.mark.parametrize("draw", ["g:text(1/0, 0, 'x')", "g:line(0, 0, 5, 5, {width = 'abc'})"])
def test_bad_draw_values_fail_the_app_not_the_host(make_app, emu_factory, draw):
    emu = emu_factory(make_app(f"function on_draw(c, g) {draw} end"))
    emu.shot()
    assert emu.errors and emu.status()["running"] is False and emu.has_text("App error")


def test_reload_saves_state_of_running_app(make_app, emu_factory):
    app = make_app(DRAW_MSG + "msg = tostring(ctx.state.n)\nfunction on_input(c) c.state.n = 42; return true end\n")
    emu = emu_factory(app)
    emu.tap(1, 1)
    emu.reload()
    assert emu.has_text("msg 42")


def test_after_cancel(make_app, emu_factory):
    app = make_app(DRAW_MSG + "function on_load(c)\n  local h = c:after(500, function() msg = 'fired'; c:invalidate() end)\n"
                   "  c:after(100, function() h:cancel() end)\nend\n")
    emu = emu_factory(app)
    emu.wait(1000)
    assert emu.has_text("msg none")


def test_after_fires_with_ctx_in_virtual_time(make_app, emu_factory):
    app = make_app(DRAW_MSG + "function on_load(c)\n  c:after(500, function(cc) msg = tostring(cc == ctx); c:invalidate() end)\nend\n")
    emu = emu_factory(app)
    emu.wait(499)
    assert emu.has_text("msg none")
    emu.wait(1)
    assert emu.has_text("msg true")


def test_back_false_offers_leave(make_app, emu_factory):
    emu = emu_factory(make_app("function on_input(c, ev) return false end"))
    assert emu.status()["pending"] is None
    emu.key("back")
    assert emu.status()["pending"]["kind"] == "leave" and emu.status()["running"] is True


def test_back_handled_does_not_offer_leave(make_app, emu_factory):
    emu = emu_factory(make_app("function on_input(c, ev) return true end"))
    emu.key("back")
    assert emu.status()["pending"] is None


def test_events_reach_on_input(make_app, emu_factory):
    app = make_app(DRAW_MSG + "seen = {}\nfunction on_input(c, ev)\n"
                   "  seen[#seen + 1] = ev.type .. ':' .. ev.phase .. ':' .. tostring(ev.gesture or ev.key)\n"
                   "  msg = table.concat(seen, ','); c:invalidate(); return true\nend\n")
    emu = emu_factory(app)
    emu.tap(10, 20)
    emu.key("up")
    assert emu.has_text("msg pointer:down:nil,pointer:up:tap,key:down:up,key:up:up")


def test_input_advances_time(make_app, emu_factory):
    emu = emu_factory(make_app(""))
    emu.tap(1, 1)
    assert emu.status()["time"] == "2026-11-20T09:00:00.350"


def test_require_loads_app_module(make_app, emu_factory):
    app = make_app(DRAW_MSG + "local x = require('lib.x')\nmsg = x.v .. ':' .. tostring(require('lib.x') == x)\n",
                   files={"lib/x.lua": "return {v=7}"})
    assert emu_factory(app).has_text("msg 7:true")


@pytest.mark.allow_errors
def test_require_error_has_module_location(make_app, emu_factory):
    emu = emu_factory(make_app("require('lib.bad')", files={"lib/bad.lua": "\nlocal t = nil; return t.x"}))
    assert "lib/bad.lua:2:" in emu.errors[0]


@pytest.mark.allow_errors
@pytest.mark.parametrize("name", ["nope", "../outside", "/etc/hosts"])
def test_require_only_from_app_folder(make_app, emu_factory, tmp_path, name):
    (tmp_path / "outside.lua").write_text("return 1", encoding="utf-8")
    emu = emu_factory(make_app(f"require('{name}')"))
    assert "module '" + name + "' not found" in emu.errors[0]


def test_sandbox_has_no_io_os(make_app, emu_factory):
    app = make_app("assert(io == nil and os == nil and dofile == nil and loadfile == nil and debug == nil)\n"
                   "assert(print == nil and collectgarbage == nil and python == nil)")
    assert emu_factory(app).status()["running"] is True


def test_state_saved_and_restored(make_app, emu_factory, tmp_path):
    app = make_app(DRAW_MSG + "function on_load(c)\n  msg = tostring(c.state.n)\n"
                   "  c.state.n = (c.state.n or 0) + 1; c.state.list = {1, 2.5, 'три'}; c.state.flag = true\n"
                   "  c:save_state()\nend\n")
    emu = emu_factory(app)
    saved = json.loads((tmp_path / "state" / "state.json").read_text(encoding="utf-8"))
    assert saved == {"n": 1, "list": [1, 2.5, "три"], "flag": True}
    emu.restart()
    assert emu.has_text("msg 1")


def test_leave_calls_on_leave_and_saves(make_app, emu_factory, tmp_path):
    emu = emu_factory(make_app("function on_leave(c) c.state.left = 'yes' end"))
    emu.leave()
    assert json.loads((tmp_path / "state" / "state.json").read_text())["left"] == "yes"
    assert emu.status()["running"] is False


def test_fresh_starts_empty(make_app, emu_factory, tmp_path):
    (tmp_path / "state").mkdir()
    (tmp_path / "state" / "state.json").write_text('{"n": 5}', encoding="utf-8")
    emu = emu_factory(make_app(DRAW_MSG + "msg = tostring(ctx.state.n)"), fresh=True)
    assert emu.has_text("msg nil")


def test_quit_leaves_after_callback(make_app, emu_factory, tmp_path):
    app = make_app("function on_input(c, ev) c:quit(); c.state.after_quit = 1; return true end\n"
                   "function on_leave(c) c.state.left = 1 end")
    emu = emu_factory(app)
    emu.tap(5, 5)
    assert emu.status()["running"] is False
    assert json.loads((tmp_path / "state" / "state.json").read_text()) == {"after_quit": 1, "left": 1}


def test_log_and_ctx_values(make_app, emu_factory):
    app = make_app("local t = ctx.clock:now()\n"
                   "ctx:log('now', t.year, t.month, t.day, t.hour, t.minute, t.second, t.wday, t.valid)\n"
                   "local b = ctx.device:battery()\n"
                   "ctx:log('api', ctx.api.major, ctx.api.minor, b.percent, b.charging)\n"
                   "local f = ctx.fonts.large\n"
                   "ctx:log('font', f.name, f.line_height, f:measure('abc') > 0, ctx.fonts:size(23).name)\n"
                   "ctx:log('millis', ctx.clock:millis(), type(ctx.device.model))\n")
    emu = emu_factory(app)
    text = "\n".join(emu.log)
    assert "now 2026 11 20 9 0 0 6 true" in text
    assert "api 2 26 87 false" in text
    assert "font large 30 true medium" in text
    assert "millis 0 string" in text


def test_status_is_json_serialisable(make_app, emu_factory):
    emu = emu_factory(make_app(DRAW_MSG))
    st = emu.status()
    json.dumps(st)
    assert st["app_id"] == "testapp" and st["net"] == "replay" and st["paused"] is False
    assert st["time"] == "2026-11-20T09:00:00.000" and st["frame_commands"] == 2


def test_save_shot_writes_png(make_app, emu_factory, tmp_path):
    emu = emu_factory(make_app(DRAW_MSG))
    path = emu.save_shot(tmp_path / "shots" / "a.png")
    assert path.exists() and path.read_bytes()[:4] == b"\x89PNG"


@pytest.mark.allow_errors
def test_zero_delay_rearm_does_not_hang_wait(emu_factory, make_app):
    app = make_app("function on_load(c) c:after(0, function() end) local f; f = function() c:after(0, f) end; f() end")
    emu = emu_factory(app)
    emu.wait("1s")
    assert any("same virtual instant" in e for e in emu.errors)
