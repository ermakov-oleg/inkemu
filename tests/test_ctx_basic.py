import json

import pytest
from conftest import IMPL

DRAW_MSG = "msg = 'none'\nfunction on_draw(c, g) g:clear(0); g:text(0, 0, 'msg ' .. tostring(msg)) end\n"


def run(make_app, emu_factory, body, **kw):
    """Runs `body` (Lua) in on_load and returns the Emu; the result is shown as `msg`."""
    return emu_factory(make_app(DRAW_MSG + "function on_load(c)\n" + body + "\nend\n", **kw.pop("app", {})), **kw)


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_storage_roundtrip_utf8(make_app, emu_factory, tmp_path):
    emu = run(
        make_app,
        emu_factory,
        "assert(ctx.storage:write('n.txt', 'Привет •'))\n"
        "msg = ctx.storage:read('n.txt') .. '|' .. ctx.storage:size('n.txt')",
    )
    assert emu.has_text("msg Привет •|")
    assert (tmp_path / "state" / "files" / "n.txt").read_text(encoding="utf-8") == "Привет •"


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_storage_binary_data(make_app, emu_factory, tmp_path):
    emu = run(
        make_app,
        emu_factory,
        "local d = '\\0\\255\\254abc\\\\x41'\nctx.storage:write('b.bin', d)\n"
        "msg = tostring(ctx.storage:read('b.bin') == d) .. ctx.storage:size('b.bin')",
    )
    assert emu.has_text("msg true10")
    assert (tmp_path / "state" / "files" / "b.bin").read_bytes() == b"\x00\xff\xfeabc\\x41"


@pytest.mark.parametrize("name", ["a/b", "a b", "", "..", "/etc/x", "a\\b"])
@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_storage_rejects_bad_names(make_app, emu_factory, name):
    lua_name = json.dumps(name)
    emu = run(
        make_app,
        emu_factory,
        f"local r, e = ctx.storage:write({lua_name}, 'x')\n"
        f"local r2, e2 = ctx.storage:read({lua_name})\nmsg = tostring(r) .. e .. tostring(r2) .. e2",
    )
    assert emu.has_text("msg nilinvalid_argumentnilinvalid_argument")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_storage_missing_is_not_found(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "local a, e = ctx.storage:read('nope')\nlocal b, e2 = ctx.storage:size('nope')\n"
        "local c3, e3 = ctx.storage:remove('nope')\n"
        "msg = tostring(a) .. e .. tostring(b) .. e2 .. e3 .. tostring(ctx.storage:exists('nope'))",
    )
    assert emu.has_text("msg nilnot_foundnilnot_foundnot_foundfalse")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_storage_list_usage_remove(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "ctx.storage:write('b.txt', 'xx'); ctx.storage:write('a.txt', 'yyy')\n"
        "local u = ctx.storage:usage()\nlocal l = ctx.storage:list()\n"
        "ctx.files:remove('a.txt')\n"
        "msg = table.concat(l, ',') .. ' ' .. u.max_files .. ' ' .. u.max_bytes .. ' ' .. u.used_files .. ' ' ..\n"
        "  u.used_bytes .. ' ' .. tostring(ctx.storage:exists('a.txt')) .. #ctx.files:list()",
    )
    assert emu.has_text("msg a.txt,b.txt 512 134217728 2 5 false1")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_files_path_and_shared_dir(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "ctx.files:write('x.txt', 'shared')\nmsg = ctx.files.path .. ' ' .. ctx.storage:read('x.txt')",
    )
    assert emu.has_text("msg /AppFiles/testapp shared")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_storage_read_offset_warns(make_app, emu_factory):
    emu = run(make_app, emu_factory, "ctx.storage:write('n.txt', 'abcdef')\nmsg = ctx.storage:read('n.txt', 0, 2)")
    assert emu.has_text("msg abcdef")
    assert any("whole file" in w for w in emu.warnings)


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_storage_quota(make_app, emu_factory, monkeypatch):
    from inkemu import ctx_basic

    monkeypatch.setattr(ctx_basic, "MAX_FILES", 2)
    monkeypatch.setattr(ctx_basic, "MAX_BYTES", 10)
    emu = run(
        make_app,
        emu_factory,
        "local r = {}\nr[1] = ctx.storage:write('a', '12345')\n"
        "r[2] = ctx.storage:write('a', '1234567890')\nr[3] = ctx.storage:write('b', 'x')\n"
        "r[4] = ctx.storage:write('c', 'x')\n"
        "msg = tostring(r[1]) .. tostring(r[2]) .. tostring(r[3]) .. tostring(r[4])",
    )
    assert emu.has_text("msg truetruenilnil")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_storage_survives_restart(make_app, emu_factory):
    app = make_app(
        DRAW_MSG + "function on_load(c)\n  msg = tostring(c.storage:read('k'))\n  c.storage:write('k', 'v')\nend\n"
    )
    emu = emu_factory(app)
    assert emu.has_text("msg nil")
    emu.restart()
    assert emu.has_text("msg v")


def test_state_with_function_is_saved_with_warning(make_app, emu_factory, tmp_path):
    app = make_app(
        DRAW_MSG + "function on_load(c)\n  c.state.f = function() end\n  c.state.ok = 1\n"
        "  c.state.nest = {deep = {g = tostring, h = 2}}\nend\n"
    )
    emu = emu_factory(app)
    emu.leave()
    saved = json.loads((tmp_path / "state" / "state.json").read_text(encoding="utf-8"))
    assert saved == {"ok": 1, "nest": {"deep": {"h": 2}}}
    assert any("state.f" in w for w in emu.warnings) and any("state.nest.deep.g" in w for w in emu.warnings)


def test_state_thread_and_nan_are_skipped(make_app, emu_factory, tmp_path):
    app = make_app(
        DRAW_MSG
        + "function on_load(c)\n  c.state.t = coroutine.create(function() end)\n  c.state.nan = 0/0\n  c.state.x = 'y'\nend\n"
    )
    emu = emu_factory(app)
    emu.leave()
    assert json.loads((tmp_path / "state" / "state.json").read_text()) == {"x": "y"}
    assert len(emu.warnings) == 2


def test_state_cycle_does_not_crash(make_app, emu_factory, tmp_path):
    emu = emu_factory(
        make_app("function on_load(c)\n  c.state.a = {}\n  c.state.a.self = c.state.a\n  c.state.k = 1\nend\n")
    )
    emu.leave()
    assert json.loads((tmp_path / "state" / "state.json").read_text()) == {"a": {}, "k": 1}
    assert any("cycle" in w and "state.a.self" in w for w in emu.warnings)


def test_state_arrays_roundtrip(make_app, emu_factory, tmp_path):
    app = make_app(
        DRAW_MSG + "function on_load(c)\n  if c.state.a then msg = #c.state.a .. c.state.a[3] .. c.state.m[2].x\n"
        "  else c.state.a = {1, 2, 'c'}; c.state.m = {{x = 1}, {x = 'two'}} end\nend\n"
    )
    emu = emu_factory(app)
    emu.restart()
    assert emu.has_text("msg 3c")
    assert json.loads((tmp_path / "state" / "state.json").read_text())["a"] == [1, 2, "c"]
    assert emu.warnings == []


def test_state_sparse_and_mixed_tables_become_dicts(make_app, emu_factory, tmp_path):
    app = make_app(
        "function on_load(c)\n  c.state.sparse = {[1] = 'a', [2] = 'b', [4] = 'd'}\n"
        "  c.state.mixed = {10, 20, name = 'n'}\n  c.state.f = {[1.5] = 'x', [true] = 'y'}\n  c.state.empty = {}\nend\n"
    )
    emu = emu_factory(app)
    emu.leave()
    saved = json.loads((tmp_path / "state" / "state.json").read_text())
    assert saved == {
        "sparse": {"1": "a", "2": "b", "4": "d"},
        "mixed": {"1": 10, "2": 20, "name": "n"},
        "f": {"1.5": "x", "true": "y"},
        "empty": {},
    }
    assert emu.warnings == []


def test_corrupt_state_starts_empty(make_app, emu_factory, tmp_path):
    (tmp_path / "state").mkdir()
    (tmp_path / "state" / "state.json").write_text("{not json", encoding="utf-8")
    emu = emu_factory(make_app(DRAW_MSG + "msg = tostring(next(ctx.state))"))
    assert emu.has_text("msg nil") and emu.warnings


def test_json_null_and_nesting(make_app, emu_factory):
    src = '{"a":null,"b":[1,{"c":"т"}],"d":true,"e":1.5}'
    emu = run(
        make_app,
        emu_factory,
        f"local v = ctx.json:decode('{src}')\n"
        "local back = ctx.json:decode(ctx.json:encode(v))\n"
        "msg = tostring(v.a == ctx.json.null) .. tostring(v.a ~= nil) .. v.b[2].c .. #v.b .. tostring(v.d) .. v.e ..\n"
        "  ' ' .. tostring(back.a == ctx.json.null) .. back.b[2].c .. ' ' .. ctx.json:encode({a = ctx.json.null})\n"
        "  .. ' ' .. ctx.json:encode({1, 'я', false})",
    )
    assert emu.has_text('msg truetrueт2true1.5 trueт {"a":null} [1,"я",false]')


def test_json_errors_and_non_string_values(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "local a, e = ctx.json:decode('{oops')\nlocal b, e2 = ctx.json:decode('NaN')\n"
        "local c3 = ctx.json:encode({f = tostring, k = 1})\n"
        "msg = tostring(a) .. e .. tostring(b) .. e2 .. c3",
    )
    assert emu.has_text('msg nilinvalid_argumentnilinvalid_argument{"k":1}')
    assert any("f" in w for w in emu.warnings)


def test_json_decode_handles_big_ints_and_unicode_escapes(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "local v = ctx.json:decode('[123456789012345678901234567890, \"\\\\u0442\"]')\nmsg = math.type(v[1]) .. v[2]",
    )
    assert emu.has_text("msg floatт")


def test_codecs_and_hashes(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "local b = ctx.base64:encode('Привет\\255')\n"
        "local parts = {b, ctx.base64:decode(b) == 'Привет\\255' and 'ok' or 'bad',\n"
        "  ctx.hex:encode('\\0\\255a'), ctx.hex:decode('00ff61') == '\\0\\255a' and 'ok' or 'bad',\n"
        "  ctx.hash:sha1('abc'), ctx.hash:sha256('abc'),\n"
        "  ctx.url:encode('a b/я?'), ctx.url:decode('a%20b%2F%D1%8F+'), #ctx.random:uuid(),\n"
        "  tostring((ctx.base64:decode('***')))}\n"
        "msg = table.concat(parts, '|')",
    )
    b64 = "0J/RgNC40LLQtdGC/w=="
    sha1 = "a9993e364706816aba3e25717850c26c9cd0d89d"
    sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    assert emu.has_text(f"msg {b64}|ok|00ff61|ok|{sha1}|{sha256}|a%20b%2F%D1%8F%3F|a b/я+|36|nil")


def test_random_uuid_is_reproducible_with_a_seed(make_app, emu_factory, tmp_path):
    app = make_app(DRAW_MSG + "function on_load(c) msg = c.random:uuid() .. ' ' .. c.random:uuid() end")
    a, b = emu_factory(app, random_seed=1), emu_factory(app, state_dir=tmp_path / "s2", random_seed=1)
    first, second = a.texts()[0]["text"].split()[1:]
    assert first != second and a.texts() == b.texts()


def test_zlib_decompress(make_app, emu_factory):
    import zlib

    data = zlib.compress("Привет мир".encode()).hex()
    emu = run(
        make_app,
        emu_factory,
        f"local z = ctx.hex:decode('{data}')\nlocal bad, e = ctx.zlib:decompress('nope')\n"
        "msg = ctx.zlib:decompress(z) .. tostring(bad) .. e",
    )
    assert emu.has_text("msg Привет мирnilinvalid_argument")


def test_zlib_decompress_gzip_and_raw_deflate(make_app, emu_factory):
    import gzip
    import zlib

    raw = zlib.compressobj(wbits=-15)
    deflated = (raw.compress("мир".encode()) + raw.flush()).hex()
    gz = gzip.compress("Привет".encode()).hex()
    emu = run(
        make_app,
        emu_factory,
        f"local a = ctx.zlib:decompress(ctx.hex:decode('{gz}'))\n"
        f"local b = ctx.zlib:decompress(ctx.hex:decode('{deflated}'))\n"
        "local c, e = ctx.zlib:decompress('')\nmsg = a .. b .. tostring(c) .. e",
    )
    assert emu.has_text("msg Приветмирnilinvalid_argument")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_utility_methods_need_a_colon(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "local ok, e = pcall(ctx.base64.encode, 'x')\nlocal ok2, e2 = pcall(ctx.storage.read, 'x')\n"
        "msg = tostring(ok) .. tostring(ok2) .. tostring(e:find('colon') ~= nil)",
    )
    assert emu.has_text("msg falsefalsetrue")


@pytest.mark.allow_errors
def test_binary_shim_keeps_app_line_in_errors(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c)\n  c.base64.encode('x')\nend"))
    assert "index.lua:2:" in emu.errors[0] and "colon" in emu.errors[0]


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_seed_files_from_fixtures(make_app, emu_factory, tmp_path):
    fx = tmp_path / "fixtures" / "files"
    fx.mkdir(parents=True)
    (fx / "cal.txt").write_text("Событие", encoding="utf-8")
    (fx / "sub").mkdir()
    emu = run(make_app, emu_factory, "msg = ctx.storage:read('cal.txt') .. #ctx.storage:list()")
    assert emu.has_text("msg Событие1")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_seed_does_not_overwrite_and_follows_fresh(make_app, emu_factory, tmp_path):
    fx = tmp_path / "fixtures" / "files"
    fx.mkdir(parents=True)
    (fx / "cal.txt").write_text("seed", encoding="utf-8")
    app = make_app(DRAW_MSG + "function on_load(c) msg = tostring(c.storage:read('cal.txt')) end")
    assert emu_factory(app, seed_files=False).has_text("msg nil")
    files = tmp_path / "state" / "files"
    files.mkdir(parents=True)
    (files / "cal.txt").write_text("user", encoding="utf-8")
    assert emu_factory(app, seed_files=True).has_text("msg user")
    assert emu_factory(app, fresh=True).has_text("msg seed")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_package_reads_data_folder(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "local r = ctx.package:open_text('w.txt')\nlocal l1 = r:read_line()\nlocal pos = r:tell()\nlocal l2 = r:read_line()\n"
        "r:seek(0)\nlocal again = r:read_line()\nlocal l3, l4 = r:read_line(), r:read_line()\nr:close()\n"
        "local n, e = ctx.package:read('absent.txt')\nlocal o, e2 = ctx.package:open_text('absent.txt')\n"
        "msg = table.concat({l1, pos, l2, again, tostring(l4), ctx.package:read('w.txt'):len(), ctx.package:size('w.txt'),\n"
        "  tostring(ctx.package:exists('w.txt')), tostring(ctx.package:exists('x')), tostring(n), e, e2}, '|')",
        app={"files": {"data/w.txt": "один\nтри\n"}},
    )
    assert emu.has_text("msg один|9|три|один|nil|16|16|true|false|nil|not_found|not_found")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_package_cannot_escape_data_folder(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "local a, e = ctx.package:read('../index.lua')\nmsg = tostring(a) .. e",
        app={"files": {"data/w.txt": "x"}},
    )
    assert emu.has_text("msg nilinvalid_argument")


def test_reader_absent_without_permission(make_app, emu_factory):
    emu = run(make_app, emu_factory, "msg = tostring(ctx.reader)")
    assert emu.has_text("msg nil")


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 7")
def test_reader_defaults_and_fixture(make_app, emu_factory, tmp_path):
    body = (
        "local cur = ctx.reader:current()\nlocal s = ctx.reader:summary()\n"
        "msg = tostring(cur and cur.title) .. #ctx.reader:books() .. #ctx.reader:all_books() .. s.streak"
    )
    manifest = {"manifest": {"permissions": ["reader"]}}
    assert run(make_app, emu_factory, body, app=manifest).has_text("msg nil000")
    fx = tmp_path / "fixtures"
    fx.mkdir()
    (fx / "reader.json").write_text(
        json.dumps(
            {
                "summary": {"streak": 4, "books": 2, "seconds": 90},
                "books": [{"title": "Идиот"}],
                "all_books": [{"title": "Идиот"}, {"title": "B"}],
                "current": {"title": "Идиот", "percent": 12},
            }
        ),
        encoding="utf-8",
    )
    assert run(make_app, emu_factory, body, app=manifest).has_text("msg Идиот124")


def test_set_orientation_resizes(make_app, emu_factory):
    app = make_app(
        DRAW_MSG + "function on_input(c, ev)\n  if ev.phase ~= 'up' then return true end\n"
        "  msg = c.screen:set_orientation('landscape') and (c.screen.width .. 'x' .. c.screen.height .. c.screen.orientation)\n"
        "  return true\nend\n"
    )
    emu = emu_factory(app)
    assert emu.shot().size == (480, 800)
    emu.tap(10, 10)
    assert emu.shot().size == (800, 480)
    assert emu.has_text("msg 800x480landscape") and emu.status()["screen"] == [800, 480]


def test_set_orientation_back_and_invalid(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "ctx.screen:set_orientation('landscape')\nlocal r, e = ctx.screen:set_orientation('sideways')\n"
        "ctx.screen:set_orientation('portrait')\nmsg = tostring(r) .. e .. ctx.screen.width .. ctx.screen.orientation",
    )
    assert emu.has_text("msg nilinvalid_argument480portrait") and emu.shot().size == (480, 800)


def test_landscape_manifest_reports_screen_size(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "msg = ctx.screen.width .. 'x' .. ctx.screen.height",
        app={"manifest": {"display": {"orientation": "landscape"}}},
    )
    assert emu.has_text("msg 800x480")


def test_capabilities_i18n_and_stubs(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "local cap = ctx.capabilities\nlocal parts = {tostring(cap.image_decode), tostring(cap.ui), tostring(cap.ttf), ctx.i18n.locale,\n"
        "  tostring(ctx.ui), tostring(ctx.system:can_open('http://x')), tostring(ctx.system:open('http://x')),\n"
        "  type(ctx.perf:info()), type(ctx.gc:count()), tostring(ctx.gc:collect()), tostring(ctx.budget:extend()),\n"
        "  ctx.frontlight:get()}\n"
        "ctx.frontlight:set(7)\nctx.display:request_refresh()\nctx.display:full_refresh()\nctx.display:full_refresh()\n"
        "parts[#parts + 1] = ctx.frontlight:get()\nmsg = table.concat(parts, '|')",
    )
    assert emu.has_text("msg true|false|false|en|nil|false|false|table|number|nil|true|0|7")
    assert emu.refresh_counts == {"request": 1, "full": 2}
    assert any("http://x" in line for line in emu.log)


def test_locale_is_configurable(make_app, emu_factory):
    app = make_app(DRAW_MSG + "function on_load(c) msg = c.i18n.locale end")
    emu = emu_factory(app)
    emu.locale = "ru"
    emu.reload()
    assert emu.has_text("msg ru")


def test_random_uuid_never_repeats_across_restarts_and_runs(make_app, emu_factory):
    app = make_app(DRAW_MSG + "function on_load(c) msg = c.random:uuid() end")
    emu = emu_factory(app)
    seen = [emu.texts()[0]["text"]]
    emu.restart()
    seen.append(emu.texts()[0]["text"])
    emu.reload()
    seen.append(emu.texts()[0]["text"])
    seen.append(emu_factory(app).texts()[0]["text"])
    assert len(set(seen)) == 4


def test_json_encode_top_level_strings_roundtrip(make_app, emu_factory):
    emu = run(
        make_app,
        emu_factory,
        "local a = ctx.json:encode('Привет')\nlocal b = ctx.json:encode('a\\\\b')\n"
        "local n = ctx.json:encode({'Привет', 'a\\\\b'})\nlocal bad, e = ctx.json:encode('\\255')\n"
        "msg = a .. b .. n .. tostring(ctx.json:decode(a) == 'Привет') .. tostring(ctx.json:decode(b) == 'a\\\\b')"
        " .. tostring(ctx.json:decode(n)[2] == 'a\\\\b') .. tostring(bad) .. e",
    )
    assert emu.has_text('msg "Привет""a\\\\b"["Привет","a\\\\b"]truetruetruenilinvalid_argument')


def test_state_with_binary_value_keeps_the_rest(make_app, emu_factory, tmp_path):
    app = make_app(
        DRAW_MSG
        + "function on_load(c)\n  if c.state.keep then msg = c.state.keep .. c.state.nest.ok .. tostring(c.state.bin)\n"
        "  else c.state.keep = 'k'; c.state.bin = '\\255\\254'; c.state['b\\255'] = 1; c.state.nest = {ok = 'o', raw = '\\200'} end\nend\n"
    )
    emu = emu_factory(app)
    emu.restart()
    assert emu.has_text("msg konil")
    saved = json.loads((tmp_path / "state" / "state.json").read_text(encoding="utf-8"))
    assert saved == {"keep": "k", "nest": {"ok": "o"}}
    assert any("state.bin" in w for w in emu.warnings) and any("state.nest.raw" in w for w in emu.warnings)
    assert any("state.b\\xff" in w for w in emu.warnings)


def test_non_table_state_warns_with_its_type(make_app, emu_factory):
    emu = emu_factory(make_app("function on_load(c) c.state = 5 end"))
    emu.leave()
    assert any("ctx.state is a number" in w for w in emu.warnings)
