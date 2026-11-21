import pytest


@pytest.fixture
def emu(make_app, emu_factory):
    return emu_factory(make_app(""))


def test_no_python_bridge_in_package_or_globals(emu):
    for expr in ("python", "package.loaded.python", "package.searchpath", "package.preload.python"):
        assert emu.lua_eval(f"return {expr}") is None, expr
    with pytest.raises(RuntimeError, match="module 'python' not found"):
        emu.lua_exec("require('python')")
    leftovers = emu.lua_eval(
        "local out = {} for k in pairs(package.loaded) do out[#out + 1] = k end "
        "table.sort(out) return table.concat(out, ' ')"
    )
    assert leftovers.split() == ["_G", "coroutine", "math", "package", "string", "table", "utf8"]


def test_sandbox(emu):
    for expr in ("io", "os", "dofile", "loadfile", "collectgarbage", "package.loadlib", "package.cpath", "python"):
        assert emu.lua_eval(f"return {expr}") is None, expr
    assert not emu.lua_eval("return debug")  # nil in the Rust core; 0.1's lupa left false
    assert emu.lua_eval("return load('return 1 + 1')()") == 2
    for expr in ("package.loaded.io", "package.loaded.os", "package.loaded.debug", "print", "__objs", "__method"):
        assert emu.lua_eval(f"return {expr}") is None, expr
    for code in ("require('os')", "require('io')", "require('debug')", "require('nonexistent')"):
        with pytest.raises(RuntimeError):
            emu.lua_exec(code)
    with pytest.raises(RuntimeError):
        emu.lua_exec("load(function() return nil end)")
    # binary chunks are refused: nil and a message
    assert emu.lua_eval("return select('#', load(string.dump(function() end)))") == 2


def test_colon_check_on_ctx_methods(emu):
    emu.lua_exec("ctx:log('x')")
    with pytest.raises(RuntimeError) as e:
        emu.lua_exec("ctx.log('x')")
    assert "bad argument #2 to 'log' (call ctx:log with a colon)" in str(e.value)


def test_colon_check_cannot_be_spoofed(emu):
    for code in (
        "ctx.log({}, 'x')",
        "__objs = {}; setmetatable(__objs, {__index = function() return 'ctx' end}); ctx.log({}, 'x')",
        "ctx.log(setmetatable({}, {__index = function() return 'ctx' end}), 'x')",
    ):
        with pytest.raises(RuntimeError, match="colon"):
            emu.lua_exec(code)


@pytest.mark.allow_errors
@pytest.mark.parametrize(
    "code, expected",
    [
        ("require('python')", "module 'python' not found"),
        ("assert(package.loaded.python).none()", "assertion failed"),
        ("package.searchpath('passwd', '/etc/?')", "field 'searchpath'"),
        ("assert(io or os or debug or python, 'no host modules')", "no host modules"),
        (
            "for _, m in pairs(package.loaded) do assert(not rawget(m, 'execute') and not rawget(m, 'popen')) end "
            "error('clean')",
            "clean",
        ),
    ],
)
def test_app_cannot_reach_python_or_os(make_app, emu_factory, code, expected):
    emu = emu_factory(make_app(f"function on_load(c) {code} end\n"))
    assert len(emu.errors) == 1 and emu.errors[0].startswith("Lua error in on_load"), emu.errors
    assert expected in emu.errors[0].split("\n")[0], emu.errors[0]
    assert emu.has_text("App error")
