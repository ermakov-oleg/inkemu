from typing import Callable

from lupa import lua55

_SANDBOX = r"""
io, os, dofile, loadfile, debug, collectgarbage = nil, nil, nil, nil, nil, nil
package.loadlib, package.cpath = nil, nil
-- keep only the preload and Lua-file searchers; the C ones read package.cpath
package.searchers[4], package.searchers[3] = nil, nil
local raw_load = load
function load(chunk, name, mode, ...)
  if type(chunk) ~= 'string' then error("bad argument #1 to 'load' (only string chunks are allowed)", 2) end
  return raw_load(chunk, name, 't', ...)
end

__objs = setmetatable({}, { __mode = 'k' })
function __method(obj_name, fn_name, pyfn)
  return function(self, ...)
    if __objs[self] ~= obj_name then
      error("bad argument #2 to '" .. fn_name .. "' (call ctx." .. obj_name .. ":" .. fn_name .. " with a colon)", 2)
    end
    return pyfn(...)
  end
end
"""


def new_runtime() -> lua55.LuaRuntime:
    L = lua55.LuaRuntime(unpack_returned_tuples=True, register_eval=False, register_builtins=False)
    L.execute(_SANDBOX)
    L.globals().python = None
    return L


def make_object(L: lua55.LuaRuntime, obj_name: str, methods: dict[str, Callable], fields: dict | None = None):
    G = L.globals()
    t = L.table()
    G.__objs[t] = obj_name
    for name, fn in methods.items():
        t[name] = G.__method(obj_name, name, fn)
    for k, v in (fields or {}).items():
        t[k] = L.table_from(v, recursive=True) if isinstance(v, (dict, list)) else v
    return t
