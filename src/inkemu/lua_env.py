import re
from collections.abc import Callable

from lupa import lua55

_SANDBOX = r"""
local STEP = ...
local objs = setmetatable({}, { __mode = 'k' })
local raw_load, traceback, xpcall, pcall = load, debug.traceback, xpcall, pcall
local error, type, tostring, pack, unpack, rawget = error, type, tostring, table.pack, table.unpack, rawget
local sethook, getinfo = debug.sethook, debug.getinfo
local co_create, co_resume, co_close = coroutine.create, coroutine.resume, coroutine.close
local HOST_SRC = getinfo(1, 'S').source

-- lupa also registers its `python` module in package.loaded, where require finds it
for _, name in ipairs({ 'io', 'os', 'debug', 'python' }) do package.loaded[name] = nil end
io, os, debug, python, dofile, loadfile, collectgarbage, print = nil, nil, nil, nil, nil, nil, nil, nil
package.loadlib, package.cpath, package.path, package.searchpath = nil, nil, '', nil
for k in pairs(package.preload) do package.preload[k] = nil end
-- lupa reads the global `debug` unprotected on every call into Lua: a raw value keeps an app's _G metatable out
debug = false
package.searchers = {}  -- the app loader is installed by the host
function load(chunk, name, mode, ...)
  if type(chunk) ~= 'string' then error("bad argument #1 to 'load' (only string chunks are allowed)", 2) end
  if name == HOST_SRC then name = '=(load)' end  -- host chunks are exempt from the budget check
  return raw_load(chunk, name, 't', ...)
end

-- Instruction budget per outermost host call: a runaway callback becomes an app error instead of hanging.
local depth, left, exhausted, budget_msg = 0, 0, false, ''
local function count_hook()
  if depth == 0 then return end
  left = left - 1
  if left >= 0 then return end
  local info = getinfo(2, 'Sl')
  if info.source == HOST_SRC then return end
  if not exhausted then
    exhausted = true
    sethook(count_hook, '', 1)  -- fire on every instruction now, so an app pcall cannot swallow the abort
  end
  local where = info.source:sub(1, 1) == '@' and info.currentline > 0 and info.short_src .. ':' .. info.currentline .. ': ' or ''
  error(where .. budget_msg, 0)
end

function coroutine.create(f)
  local co = co_create(f)
  sethook(co, count_hook, '', exhausted and 1 or STEP)
  return co
end

function coroutine.wrap(f)
  local co = coroutine.create(f)
  return function(...)
    local r = pack(co_resume(co, ...))
    if r[1] then return unpack(r, 2, r.n) end
    co_close(co)
    error(r[2], type(r[2]) == 'string' and 2 or 0)
  end
end

local function guarded(units, f, ...)
  if depth > 0 then return xpcall(f, ...) end
  depth, left, exhausted = 1, units, false
  budget_msg = 'callback exceeded the instruction budget (' .. units * STEP .. ' instructions) - infinite loop?'
  sethook(count_hook, '', STEP)
  local r = pack(xpcall(f, ...))
  sethook()
  depth = 0
  if r[1] and exhausted then return false, traceback(budget_msg, 1) end
  return unpack(r, 1, r.n)
end

local function method(obj_name, fn_name, fn)
  local where = obj_name == 'ctx' and 'ctx' or 'ctx.' .. obj_name
  local msg = "bad argument #2 to '" .. fn_name .. "' (call " .. where .. ":" .. fn_name .. " with a colon)"
  if type(fn) == 'function' then
    return function(self, ...)
      if objs[self] ~= obj_name then error(msg, 2) end
      return fn(...)
    end
  end
  -- a Python exception must reach the app as a string error at the app's line, not as a Python object
  return function(self, ...)
    if objs[self] ~= obj_name then error(msg, 2) end
    local r = pack(pcall(fn, ...))
    if r[1] then return unpack(r, 2, r.n) end
    error(type(r[2]) == 'string' and r[2] or tostring(r[2]), 2)
  end
end

local function handler(e)
  return traceback(type(e) == 'string' and e or tostring(e), 2)
end

return method,
  function(t, obj_name) objs[t] = obj_name end,
  function(units, f, ...) return guarded(units, f, handler, ...) end,
  function(src, chunkname) return raw_load(src, chunkname, 't') end,
  rawget
"""


# ~0.7 s of pure Lua on a laptop, far beyond what the reader's CPU runs in one callback; counted, so deterministic
INSTRUCTION_BUDGET = 200_000_000
HOOK_STEP = 10_000

_HOST_FRAME = re.compile(r"""\[string "<python>"\]|^\s*\[C\]: in upvalue 'xpcall'$""")


class Runtime(lua55.LuaRuntime):
    """LuaRuntime that owns the sandbox helper closures, so they die with it."""


def _no_attributes(obj, name, is_setting):
    # app Lua must never walk a Python object (obj.__class__ ... -> os); host Lua never needs attributes
    raise AttributeError(f"access to Python attribute {name!r} is not allowed")


def new_runtime() -> Runtime:
    L = Runtime(
        unpack_returned_tuples=True, register_eval=False, register_builtins=False, attribute_filter=_no_attributes
    )
    L._method, L._register, L._xpcall, L._load, L._rawget = L.execute(_SANDBOX, HOOK_STEP)
    return L


def protected_call(L: Runtime, fn, *args) -> tuple[bool, tuple]:
    """Call a Lua function like xpcall: (True, results) or (False, (message_with_traceback,))."""
    r = L._xpcall(max(INSTRUCTION_BUDGET // HOOK_STEP, 1), fn, *args)
    if not isinstance(r, tuple):
        r = (r,)
    if r[0]:
        return True, r[1:]
    msg = r[1] if len(r) > 1 and isinstance(r[1], str) else str(r[1:])
    head, sep, tb = msg.partition("\nstack traceback:")
    return False, (head + sep + "\n".join(ln for ln in tb.split("\n") if not _HOST_FRAME.search(ln)),)


def load_chunk(L: Runtime, src: str, chunkname: str):
    """Compile text Lua code; returns (function, None) or (None, error message)."""
    r = L._load(src, chunkname)
    if isinstance(r, tuple):
        return r[0], (r[1] if len(r) > 1 else None)
    return r, None


def add_methods(L: Runtime, t, obj_name: str, methods: dict[str, Callable]) -> None:
    for name, fn in methods.items():
        t[name] = L._method(obj_name, name, fn)


def make_object(L: Runtime, obj_name: str, methods: dict[str, Callable], fields: dict | None = None):
    t = L.table()
    L._register(t, obj_name)
    add_methods(L, t, obj_name, methods)
    for k, v in (fields or {}).items():
        t[k] = L.table_from(v, recursive=True) if isinstance(v, (dict, list)) else v
    return t
