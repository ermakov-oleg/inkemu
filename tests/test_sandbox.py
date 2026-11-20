import pytest
from lupa import LuaError

from inkemu.lua_env import new_runtime


def test_no_python_bridge_in_package_or_globals():
    L = new_runtime()
    for expr in ("python", "package.loaded.python", "package.searchpath", "package.preload.python"):
        assert L.eval(expr) is None, expr
    with pytest.raises(LuaError, match="module 'python' not found"):
        L.execute("require('python')")
    leftovers = L.execute("local out = {} for k in pairs(package.loaded) do out[#out + 1] = k end "
                          "table.sort(out) return table.concat(out, ' ')")
    assert leftovers.split() == ["_G", "coroutine", "math", "package", "string", "table", "utf8"]


def test_leaked_python_object_has_no_attributes():
    L = new_runtime()
    L.globals().leak = (lambda: 1)
    for code in ("return leak.__class__", "return leak.__globals__", "leak.x = 1"):
        with pytest.raises(Exception):
            L.execute(code)


@pytest.mark.allow_errors
@pytest.mark.parametrize("code, expected", [
    ("require('python')", "module 'python' not found"),
    ("assert(package.loaded.python).none()", "assertion failed"),
    ("package.searchpath('passwd', '/etc/?')", "field 'searchpath'"),
    ("assert(io or os or debug or python, 'no host modules')", "no host modules"),
    ("for _, m in pairs(package.loaded) do assert(not rawget(m, 'execute') and not rawget(m, 'popen')) end "
     "error('clean')", "clean"),
])
def test_app_cannot_reach_python_or_os(make_app, emu_factory, code, expected):
    emu = emu_factory(make_app(f"function on_load(c) {code} end\n"))
    assert len(emu.errors) == 1 and emu.errors[0].startswith("Lua error in on_load"), emu.errors
    assert expected in emu.errors[0].split("\n")[0], emu.errors[0]
    assert emu.has_text("App error")
