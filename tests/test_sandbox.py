import pytest

NO_PACKAGE = "firmware 26112001 inventory line 21: no package global"


@pytest.fixture
def emu(make_app, emu_factory):
    return emu_factory(make_app(""))


def test_no_python_bridge_in_package_or_globals(emu):
    for expr in ("python", "package"):
        assert emu.lua_eval(f"return {expr}") is None, (expr, NO_PACKAGE)
    with pytest.raises(RuntimeError, match="module 'python' not found"):
        emu.lua_exec("require('python')")
    with pytest.raises(RuntimeError, match="module 'package' not found"):
        emu.lua_exec("require('package')")
    for lib in ("_G", "coroutine", "math", "string", "table", "utf8"):
        assert emu.lua_eval(f"return type(require('{lib}'))") == "table", (lib, NO_PACKAGE)


def test_sandbox(emu):
    for expr in ("io", "os", "dofile", "loadfile", "collectgarbage", "package", "python"):
        assert emu.lua_eval(f"return {expr}") is None, (expr, NO_PACKAGE)
    assert not emu.lua_eval("return debug")  # nil in the Rust core; 0.1's lupa left false
    assert emu.lua_eval("return load") is None, "firmware 26112001 inventory: no load, loadstring, dofile, loadfile"
    for expr in ("print", "__objs", "__method"):
        assert emu.lua_eval(f"return {expr}") is None, expr
    for code in ("require('os')", "require('io')", "require('debug')", "require('nonexistent')"):
        with pytest.raises(RuntimeError):
            emu.lua_exec(code)


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
        ("assert(package.loaded.python).none()", "attempt to index a nil value (global 'package')"),
        ("package.searchpath('passwd', '/etc/?')", "attempt to index a nil value (global 'package')"),
        ("assert(io or os or debug or python, 'no host modules')", "no host modules"),
        (
            "for _, n in ipairs({'_G', 'coroutine', 'math', 'string', 'table', 'utf8'}) do local m = require(n) "
            "assert(not rawget(m, 'execute') and not rawget(m, 'popen')) end error('clean')",
            "clean",
        ),
    ],
)
def test_app_cannot_reach_python_or_os(make_app, emu_factory, code, expected):
    emu = emu_factory(make_app(f"function on_load(c) {code} end\n"))
    assert len(emu.errors) == 1 and emu.errors[0].startswith("Lua error in on_load"), emu.errors
    assert expected in emu.errors[0].split("\n")[0], (emu.errors[0], NO_PACKAGE)
    assert emu.has_text("App error")
