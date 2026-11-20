from typing import Callable

from lupa import lua55

_SANDBOX = r"""
local objs = setmetatable({}, { __mode = 'k' })
local raw_load = load

for _, name in ipairs({ 'io', 'os', 'debug' }) do package.loaded[name] = nil end
io, os, debug, dofile, loadfile, collectgarbage, print = nil, nil, nil, nil, nil, nil, nil
package.loadlib, package.cpath, package.path = nil, nil, ''
package.searchers = {}  -- the app loader is installed by the host
function load(chunk, name, mode, ...)
  if type(chunk) ~= 'string' then error("bad argument #1 to 'load' (only string chunks are allowed)", 2) end
  return raw_load(chunk, name, 't', ...)
end

return function(obj_name, fn_name, pyfn)
  return function(self, ...)
    if objs[self] ~= obj_name then
      error("bad argument #2 to '" .. fn_name .. "' (call ctx." .. obj_name .. ":" .. fn_name .. " with a colon)", 2)
    end
    return pyfn(...)
  end
end, function(t, obj_name) objs[t] = obj_name end
"""

_helpers: dict[int, tuple] = {}


def new_runtime() -> lua55.LuaRuntime:
    L = lua55.LuaRuntime(unpack_returned_tuples=True, register_eval=False, register_builtins=False)
    _helpers[id(L)] = (L, *L.execute(_SANDBOX))
    L.globals().python = None
    return L


def make_object(L: lua55.LuaRuntime, obj_name: str, methods: dict[str, Callable], fields: dict | None = None):
    _, method, register = _helpers[id(L)]
    t = L.table()
    register(t, obj_name)
    for name, fn in methods.items():
        t[name] = method(obj_name, name, fn)
    for k, v in (fields or {}).items():
        t[k] = L.table_from(v, recursive=True) if isinstance(v, (dict, list)) else v
    return t
