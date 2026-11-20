import pytest

from inkemu import daemon, lua_env

LOOP = "while true do end"
BUDGET_MSG = "callback exceeded the instruction budget"


@pytest.fixture(autouse=True)
def small_budget(monkeypatch):
    monkeypatch.setattr(lua_env, "INSTRUCTION_BUDGET", 1_000_000)


def _one_budget_error(emu, where: str) -> str:
    assert len(emu.errors) == 1, emu.errors
    err = emu.errors[0]
    assert err.startswith(f"Lua error in {where}") and BUDGET_MSG in err and "infinite loop?" in err, err
    assert emu.has_text("App error") and not emu.running
    return err


@pytest.mark.allow_errors
@pytest.mark.parametrize("where, code, act", [
    ("on_load", f"function on_load(c) {LOOP} end", None),
    ("on_draw", f"function on_draw(c, g) {LOOP} end", None),
    ("on_input", f"function on_input(c, ev) {LOOP} end", lambda e: e.tap(10, 10)),
    ("ctx:after callback", f"function on_load(c) c:after(100, function() {LOOP} end) end", lambda e: e.wait(500)),
    ("on_tick", f"function on_load(c) c:set_tick(1000) end function on_tick(c) {LOOP} end", lambda e: e.wait("2s")),
])
def test_runaway_callback_is_an_app_error(make_app, emu_factory, where, code, act):
    emu = emu_factory(make_app(code + "\n"))
    if act:
        act(emu)
    err = _one_budget_error(emu, where)
    assert "index.lua:1: " + BUDGET_MSG in err
    emu.wait("1s")  # the host keeps running


@pytest.mark.allow_errors
def test_runaway_lock_screen(make_app, emu_factory):
    app = make_app("function on_draw(c, g) g:text(0, 0, 'app') end\n",
                   manifest={"lockscreen": {"enabled": True, "entry": "lock.lua"}},
                   files={"lock.lua": f"function on_draw(c, g) {LOOP} end\n"})
    emu = emu_factory(app)
    emu.lock()
    _one_budget_error(emu, "on_draw")


@pytest.mark.allow_errors
@pytest.mark.parametrize("code", [
    f"while true do pcall(function() {LOOP} end) end",
    f"return pcall(function() {LOOP} end)",
    f"while true do xpcall(function() {LOOP} end, function(e) return e end) end",
    f"coroutine.wrap(function() {LOOP} end)()",
    f"local co = coroutine.create(function() {LOOP} end) while true do coroutine.resume(co) end",
    f"pcall(coroutine.wrap(function() {LOOP} end)) {LOOP}",
    f"setmetatable({{}}, {{__index = function() {LOOP} end}}).x = nil; local _ = setmetatable({{}}, {{__index = function() {LOOP} end}}).x",
])
def test_app_cannot_swallow_the_abort(make_app, emu_factory, code):
    emu = emu_factory(make_app(f"function on_load(c) {code} end\n"))
    err = _one_budget_error(emu, "on_load")
    assert "error in error handling" not in err


@pytest.mark.allow_errors
def test_nested_callback_abort_is_reported_once(make_app, emu_factory):
    emu = emu_factory(make_app(f"function on_load(c) local l = c.layers:create(10, 10) "
                               f"for i = 1, 10 do l:draw(function(g) {LOOP} end) end end\n"))
    _one_budget_error(emu, "layer:draw callback")


def test_budget_is_per_callback_and_resets(make_app, emu_factory):
    # ~60 % of the budget per on_input call (down and up of three taps); they must not add up
    app = make_app("n = 0\nfunction on_input(c, ev) for i = 1, 600000 do end n = n + 1 c:invalidate() return true end\n"
                   "function on_draw(c, g) g:text(0, 0, 'n ' .. n) end\n")
    emu = emu_factory(app)
    for _ in range(3):
        emu.tap(10, 10)
    assert emu.has_text("n 6")


@pytest.mark.allow_errors
def test_load_cannot_fake_a_host_chunk(make_app, emu_factory):
    app = make_app(f"function on_load(c) load('{LOOP}', '<python>')() end\n")
    emu = emu_factory(app)
    _one_budget_error(emu, "on_load")



@pytest.mark.allow_errors
def test_daemon_survives_runaway_and_stops(make_app, emu_factory):
    emu = emu_factory(make_app(f"function on_input(c, ev) {LOOP} end\nfunction on_draw(c, g) g:text(0, 0, 'hi') end\n"))
    r = daemon.execute(emu, "tap-text", {"text": "hi"})
    assert r["ok"] and BUDGET_MSG in r["errors"][0]
    assert daemon.execute(emu, "stop")["ok"]


def test_strict_globals_metatable_does_not_break_callbacks(make_app, emu_factory):
    # a strict.lua-style _G must not turn the host's lookup of undefined callbacks into an error or a hang
    app = make_app("function on_draw(c, g) g:text(0, 0, 'ok') end\n"
                   "setmetatable(_G, {__index = function(_, k) error('undefined global ' .. k, 2) end})\n")
    emu = emu_factory(app)
    emu.tap(10, 10)
    emu.wait("1s")
    assert emu.has_text("ok")
