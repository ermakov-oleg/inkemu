import json

BLOCK = {"enabled": True, "entry": "lockscreen.lua", "interval_sec": 60, "net": True}
APP = """
loaded = false
function on_load(c) c.state.n = (c.state.n or 0) + 1; c.storage:write('app_loads', tostring(c.state.n)) end
function on_resume(c) c.storage:write('resumed', 'yes') end
function on_leave(c) c.storage:write('left', 'yes') end
function on_draw(c, g) g:clear(0); g:text(0, 0, 'app screen ' .. c.state.n) end
function on_input(c, ev) c.storage:write('input', 'seen'); return true end
"""
LOCK = """
local seen
function on_load(c)
  local n = tonumber(c.storage:read('count') or '0') + 1
  c.storage:write('count', tostring(n))
  seen = {reason = c.lock.reason, n = n, net = c.net ~= nil, iv = c.lock.interval_sec, el = c.lock.elapsed_sec}
  %s
end
function on_draw(c, g) g:clear(0); g:text(0, 0, 'reason ' .. seen.reason); g:text(0, 30, 'n ' .. seen.n .. ' net ' .. tostring(seen.net) .. ' iv ' .. seen.iv .. ' el ' .. seen.el) end
"""


def make(make_app, extra="", block=BLOCK, lock=None, **kw):
    manifest = {"lockscreen": block} if block is not None else {}
    manifest.update(kw.pop("manifest", {}))
    return make_app(APP, manifest=manifest, files={"lockscreen.lua": (lock or LOCK) % extra})


def test_lock_draws_lockscreen(make_app, emu_factory, tmp_path):
    emu = emu_factory(make(make_app))
    emu.lock()
    assert emu.locked and emu.status()["locked"]
    assert (tmp_path / "state" / "files" / "left").read_text() == "yes"
    assert emu.has_text("reason lock") and not emu.has_text("app screen")
    assert emu.lock_wakes == 0


def test_lock_wakes_on_interval(make_app, emu_factory):
    emu = emu_factory(make(make_app))
    emu.lock()
    emu.wait("3m")
    assert emu.lock_wakes == 3
    assert emu.has_text("reason timer") and emu.has_text("n 4 ") and emu.has_text("el 60")


def test_set_interval_changes_next_wake(make_app, emu_factory):
    emu = emu_factory(make(make_app, "c.lock:set_interval(120)"))
    emu.lock()
    emu.wait("4m")
    assert emu.lock_wakes == 2
    assert emu.has_text("iv 120")


def test_lock_net_follows_manifest(make_app, emu_factory):
    emu = emu_factory(make(make_app))
    emu.lock()
    assert emu.has_text("net true")
    emu.unlock()
    emu = emu_factory(make(make_app, block={**BLOCK, "net": False}))
    emu.lock()
    assert emu.has_text("net false")


def test_unlock_restarts_app(make_app, emu_factory, tmp_path):
    emu = emu_factory(make(make_app))
    emu.lock()
    emu.wait("2m")
    emu.unlock()
    files = tmp_path / "state" / "files"
    assert not emu.locked and emu.has_text("app screen 2")
    assert (files / "app_loads").read_text() == "2" and (files / "resumed").read_text() == "yes"
    emu.wait("5m")
    assert emu.lock_wakes == 2 and emu.has_text("app screen")


def test_lockscreen_api_absent_without_block(make_app, emu_factory):
    app = make_app(
        "function on_load(c) c.state.has = c.lockscreen ~= nil end\n"
        "function on_draw(c, g) g:text(0, 0, 'has ' .. tostring(c.state.has)) end"
    )
    assert emu_factory(app).has_text("has false")


def test_lockscreen_api_default_persists(make_app, emu_factory, tmp_path):
    app = make_app(
        "function on_load(c) c.state.before = c.lockscreen:is_default(); c.lockscreen:set_default() end\n"
        "function on_draw(c, g) g:text(0, 0, 'b ' .. tostring(c.state.before) .. ' a ' .. tostring(c.lockscreen:is_default())) end",
        manifest={"lockscreen": BLOCK},
    )
    emu = emu_factory(app)
    assert emu.has_text("b false a true")
    assert json.loads((tmp_path / "state" / "lock.json").read_text())["is_default"] is True
    emu.restart()
    assert emu.has_text("b true a true")


def test_input_ignored_while_locked(make_app, emu_factory, tmp_path):
    emu = emu_factory(make(make_app))
    emu.lock()
    emu.tap(10, 10)
    emu.key("ok")
    assert not (tmp_path / "state" / "files" / "input").exists()
    assert any("input ignored" in ln for ln in emu.log)


def test_wake_waits_for_net(make_app, emu_factory, tmp_path):
    d = tmp_path / "fixtures" / "net"
    d.mkdir(parents=True)
    (d / "a.json").write_text(
        json.dumps(
            {
                "request": {"method": "GET", "url": "http://x.test/a"},
                "match": "exact",
                "response": {"status": 200, "headers": {}, "body": "ok", "delay_ms": 5000},
            }
        )
    )
    req = "c.net:request({url = 'http://x.test/a'}, function(c, r) seen.reason = 'got ' .. r.body end)"
    emu = emu_factory(make(make_app, req, manifest={"permissions": ["net"]}))
    emu.lock()
    emu.wait("60s")
    assert emu.lock_wakes == 1 and not emu.has_text("reason timer")
    emu.wait("4999ms")
    assert not emu.has_text("reason got")
    emu.wait(1)
    assert emu.has_text("reason got ok")


def slow_net(tmp_path):
    d = tmp_path / "fixtures" / "net"
    d.mkdir(parents=True)
    (d / "z.json").write_text(
        json.dumps(
            {
                "request": {"method": "GET", "url": "http://x.test/z"},
                "match": "exact",
                "response": {"status": 200, "headers": {}, "body": "ok", "delay_ms": 60000},
            }
        )
    )


REQ = "c.net:request({url = 'http://x.test/z'}, function() end)"


def test_wake_done_draws_before_net(make_app, emu_factory, tmp_path):
    slow_net(tmp_path)
    emu = emu_factory(
        make(make_app, REQ + "\n  c:after(2000, function() c.lock:done() end)", manifest={"permissions": ["net"]})
    )
    emu.lock()
    emu.wait("61s")
    assert not emu.has_text("reason timer")
    emu.wait("1s")
    assert emu.has_text("reason timer")


def test_wake_waits_at_most_30s(make_app, emu_factory, tmp_path):
    slow_net(tmp_path)
    emu = emu_factory(make(make_app, REQ, manifest={"permissions": ["net"]}))
    emu.lock()
    emu.wait("89s")
    assert not emu.has_text("reason timer")
    emu.wait("1s")
    assert emu.has_text("reason timer")


def test_lock_without_block_is_blank(make_app, emu_factory, tmp_path):
    emu = emu_factory(make(make_app, block=None))
    emu.lock()
    assert emu.locked and emu.texts() == []
    assert (tmp_path / "state" / "files" / "left").exists()
    emu.wait("10m")
    assert emu.lock_wakes == 0
    emu.unlock()
    assert emu.has_text("app screen")


def test_lock_state_does_not_overwrite_app_state(make_app, emu_factory):
    emu = emu_factory(make(make_app, "c.state.x = 1; c:save_state()"))
    emu.lock()
    emu.unlock()
    assert emu.has_text("app screen 2")
