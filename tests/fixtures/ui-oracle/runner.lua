-- Replays one oracle scenario against ctx.ui and keeps one JSON object per step in RESULT (one per line).
-- The steps come from `steps.lua` (the scenario's `input.steps`, written as a Lua table by the test). Everything runs
-- in the first on_draw, because begin_frame needs the frame's canvas. Canvas calls are recorded by wrapping the real
-- g's methods in place, so the stream holds the library's exact arguments whatever the core does with them. A call
-- the core refuses with a Lua error is kept apart in the step's `host_errors`, so the library carries on as it would
-- with the oracle's canvas, which accepts anything.
local input = require("steps")

local DRAW_OPS = {
  "clear", "rect", "round_rect", "line", "circle", "arc", "polygon", "polyline", "text", "text_box", "image",
  "pattern", "pattern_rect", "fill_pattern", "pixel",
}

local G, FONT_NAMES = nil, {}
-- image objects by the scenario's "@img:WxH" marker and back, and the stub's edit objects
local IMAGES, IMAGE_NAMES, EDIT_NAMES = {}, {}, setmetatable({}, { __mode = "k" })
local stream, counts, host_errors = {}, {}, {}

-- JSON ---------------------------------------------------------------------------------------------------------

local function str(s)
  return '"' .. string.gsub(s, '[\0-\31"\\]', function(c)
    if c == '"' then return '\\"' end
    if c == "\\" then return "\\\\" end
    if c == "\n" then return "\\n" end
    if c == "\t" then return "\\t" end
    return string.format("\\u%04x", string.byte(c))
  end) .. '"'
end

local function num(v)
  if math.type(v) == "integer" then return string.format("%d", v) end
  if v ~= v then return '"NaN"' end
  if v == math.huge then return '"Infinity"' end
  if v == -math.huge then return '"-Infinity"' end
  local s = string.format("%.17g", v)
  if not string.find(s, "[%.eEn]") then s = s .. ".0" end
  return s
end

local value

local function list(t, from, to)
  local parts = {}
  for i = from, to do parts[#parts + 1] = value(t[i]) end
  return "[" .. table.concat(parts, ",") .. "]"
end

-- an object from (key, json) pairs, keys sorted
local function object(fields)
  local keys = {}
  for k in pairs(fields) do keys[#keys + 1] = k end
  table.sort(keys)
  local parts = {}
  for i, k in ipairs(keys) do parts[i] = str(k) .. ":" .. fields[k] end
  return "{" .. table.concat(parts, ",") .. "}"
end

value = function(v)
  local t = type(v)
  if v == nil then return "null" end
  if t == "boolean" then return v and "true" or "false" end
  if t == "number" then return num(v) end
  if t == "string" then return str(v) end
  if t == "function" then return '"@fn"' end
  if t ~= "table" then return str("@" .. t) end
  if FONT_NAMES[v] then return str("@font:" .. FONT_NAMES[v]) end
  if rawequal(v, G) then return '"@g"' end
  if IMAGE_NAMES[v] then return str(IMAGE_NAMES[v]) end
  if rawequal(v, ctx) then return '"@ctx"' end
  if EDIT_NAMES[v] then return '"@ed"' end
  local n, count = #v, 0
  for _ in pairs(v) do count = count + 1 end
  if n > 0 and n == count then return list(v, 1, n) end
  local fields = {}
  for k, x in pairs(v) do fields[tostring(k)] = value(x) end
  return object(fields)
end

local function strip(msg)
  return (string.gsub(tostring(msg), "^[^\n]-:%d+: ", ""))
end

-- the JSON list of every return value, or nil and the JSON error message when the call raised
local function returns(f, ...)
  local r = table.pack(pcall(f, ...))
  if r[1] then return list(r, 2, r.n) end
  return nil, str(strip(r[2]))
end

-- every return value, or {error} when the call raised
local function call(f, ...)
  local ret, err = returns(f, ...)
  return ret or object({ error = err })
end

-- recording ----------------------------------------------------------------------------------------------------

local function record(entry, op)
  stream[#stream + 1] = entry
  counts[op] = (counts[op] or 0) + 1
end

local function patch(g)
  for _, name in ipairs(DRAW_OPS) do
    local orig = g[name]
    if type(orig) == "function" then
      g[name] = function(self, ...)
        record(object({ op = str(name), a = list(table.pack(...), 1, select("#", ...)) }), name)
        local r = table.pack(pcall(orig, self, ...))
        if not r[1] then
          host_errors[#host_errors + 1] = str(name .. ": " .. strip(r[2]))
          return
        end
        return table.unpack(r, 2, r.n)
      end
    end
  end
end

-- a draw callback for control `id`: region draw(g, r, st) or list draw(g, r, i, st)
local function draw_fn(kind, id)
  return function(g, r, a, b)
    local i, st = nil, a
    if b ~= nil or math.type(a) == "integer" then i, st = a, b end
    record(object({
      op = '"@draw"', id = value(id), r = value(r), st = value(st), i = i ~= nil and value(i) or nil,
      g_is_frame_canvas = value(rawequal(g, G)),
    }), "@draw")
    if kind == "@rect" then g:rect(r.x, r.y, r.w, r.h, { color = 15 }) end
    if kind == "@throw" then error("draw boom " .. tostring(id)) end
  end
end

local MARKERS = { ["@rec"] = true, ["@rect"] = true, ["@throw"] = true }

-- an image object of `w` x `h` from the PNG the test put in the package, one per size
local function image(w, h)
  local key = w .. "x" .. h
  if IMAGES[key] == nil then
    local img = assert(ctx.package:image("img_" .. key .. ".png"))
    IMAGES[key] = img
    IMAGE_NAMES[img] = "@img:" .. key
  end
  return IMAGES[key]
end

-- a host callback (a field's on_change or on_end) that only records its arguments
local function callback(id, key)
  return function(...)
    record(object({ op = '"@cb"', id = value(id), key = value(key), a = list(table.pack(...), 1, select("#", ...)) }),
      "@cb")
  end
end

-- the scenario's value markers turned into real values
local function conv(v, id, key)
  if type(v) == "string" then
    local font = string.match(v, "^@font:(%a+)$")
    if font then return ctx.fonts[font] end
    local w, h = string.match(v, "^@img:(%d+)x(%d+)$")
    if w then return image(w, h) end
    if MARKERS[v] then return draw_fn(v, id) end
    if v == "@none" or v == "@nil" then return nil end
    if v == "@g" then return G end
    if v == "@fn" then return function() end end
    if v == "@ctx" then return ctx end
    if v == "@cb" then return callback(id, key) end
    if v == "@nan" then return 0 / 0 end
    if v == "@inf" then return math.huge end
    return v
  end
  if type(v) ~= "table" then return v end
  local here = type(v.id) == "string" and v.id or id
  local t = {}
  for k, x in pairs(v) do t[k] = conv(x, here, k) end
  return t
end

-- the docked input as the oracle's stub: `ctx.input:edit` records its options and answers by INPUT_MODE; the steps
-- `edit_type` and `edit_end` play the host's part on the live edit
local INPUT_MODE, EDITS = "ok", {}

local function stub_edit(_, o)
  record(object({ op = '"@input_edit"', a = value(o) }), "@input_edit")
  if INPUT_MODE ~= "ok" then return nil, INPUT_MODE end
  local ed = { t = o.text or "", alive = true, o = o }
  function ed.text(self) return self.alive and self.t or nil end
  function ed.close(self)
    record(object({ op = '"@ed_close"', alive = value(self.alive) }), "@ed_close")
    if not self.alive then return nil end
    self.alive = false
    return self.t
  end
  EDIT_NAMES[ed] = true
  EDITS[#EDITS + 1] = ed
  return ed
end

local function live_edit()
  for i = #EDITS, 1, -1 do
    if EDITS[i].alive then return EDITS[i] end
  end
end

-- `edit_type` / `edit_end`: the error of a callback that raised, or nil
local function host_edit(s)
  local ed = live_edit()
  if ed == nil then return '"@no live edit"' end
  local o, r = ed.o, nil
  if s.op == "edit_type" then
    ed.t = s.text
    if type(o.on_change) == "function" then r = table.pack(pcall(o.on_change, ctx, s.text)) end
  else
    if s.text ~= nil then ed.t = s.text end
    ed.alive = false
    if type(o.on_end) == "function" then r = table.pack(pcall(o.on_end, ctx, ed.t, s.how)) end
  end
  if r and not r[1] then return str(strip(r[2])) end
end

-- layout results -----------------------------------------------------------------------------------------------

local BOX_KEYS = { x = true, y = true, w = true, h = true, overflow = true }

local function box(t)
  if type(t) ~= "table" then return value(t) end
  local fields = { x = value(t.x), y = value(t.y), w = value(t.w), h = value(t.h) }
  if t.overflow ~= nil then fields.overflow = value(t.overflow) end
  if #t > 0 then
    local kids = {}
    for i = 1, #t do kids[i] = box(t[i]) end
    fields.c = "[" .. table.concat(kids, ",") .. "]"
  end
  local keys = {}
  for k in pairs(t) do
    if type(k) == "string" and not BOX_KEYS[k] then keys[#keys + 1] = k end
  end
  if #keys > 0 then
    table.sort(keys)
    fields.keys = list(keys, 1, #keys)
  end
  return object(fields)
end

local function path_of(root, target)
  local function walk(t, path)
    if rawequal(t, target) then return path end
    for i = 1, #t do
      if type(t[i]) == "table" then
        local p = walk(t[i], path .. "." .. i)
        if p then return p end
      end
    end
  end
  for i = 1, #root do
    if type(root[i]) == "table" then
      local p = walk(root[i], "." .. i)
      if p then return p end
    end
  end
end

local function layout_result(r)
  if not r[1] then return object({ error = str(strip(r[2])) }) end
  local b = r[2]
  if type(b) ~= "table" then return object({ ret = list(r, 2, r.n) }) end
  local ids = {}
  for k, v in pairs(b) do
    if type(k) == "string" and not BOX_KEYS[k] then ids[k] = value(path_of(b, v)) end
  end
  return object({ box = box(b), ids = object(ids), n_returns = value(r.n - 1) })
end

-- steps --------------------------------------------------------------------------------------------------------

local objs = {}

-- what a frame step passes to begin_frame
local CANVAS_ARGS = { none = function() return nil end, table = function() return {} end, five = function() return 5 end }

local function canvas_arg(marker)
  if marker == nil then return G end
  local make = CANVAS_ARGS[marker]
  if make == nil then error("runner: unknown canvas marker " .. tostring(marker)) end
  return make()
end

local function run(i, s)
  stream, counts, host_errors = {}, {}, {}
  local out = { i = value(i), op = str(s.op) }
  local u = objs[s.ui or "ui"]
  if s.op == "input_mode" then
    INPUT_MODE = s.mode
    return object(out)
  end
  if s.op == "create" then
    local r
    if s.raw then
      r = table.pack(pcall(ctx.ui.create, ctx.ui, table.unpack(s.args, 1, #s.args)))
    else
      r = table.pack(pcall(ctx.ui.create, ctx.ui, conv(s.opts)))
    end
    if r[1] and type(r[2]) == "table" then
      objs[s.name or "ui"] = r[2]
      out.ret = '"ui object"'
    elseif r[1] then
      out.ret = list(r, 2, r.n)
    else
      out.ret = object({ error = str(strip(r[2])) })
    end
    return object(out)
  end
  if u == nil then return object({ i = value(i), op = str(s.op), error = '"@no such ui object"' }) end
  if s.op == "frame" or s.op == "begin" then
    out.begin = call(u.begin_frame, u, canvas_arg(s.g))
  end
  if s.calls then
    local calls = {}
    for k, c in ipairs(s.calls) do
      local e = { m = str(c.m) }
      if c.id ~= nil then e.id = value(c.id) end
      if c.m == "@g" then
        local args = conv(c.args)
        local r = table.pack(pcall(G[c.op], G, table.unpack(args, 1, #c.args)))
        if not r[1] then e.error = str(strip(r[2])) end
      elseif u[c.m] == nil then
        e.error = '"@no such method"'
      else
        if c.args then
          e.ret, e.error = returns(u[c.m], u, table.unpack(conv(c.args, c.id), 1, #c.args))
        else
          e.ret, e.error = returns(u[c.m], u, c.id, conv(c.o, c.id))
        end
      end
      calls[k] = object(e)
    end
    out.calls = "[" .. table.concat(calls, ",") .. "]"
  end
  if s.op == "frame" or s.op == "end" then out["end"] = call(u.end_frame, u) end
  if s.op == "event" then
    out.ret = call(u.handle_event, u, conv(s.ev))
    if #stream > 0 then out.side_effects = "[" .. table.concat(stream, ",") .. "]" end
  end
  if s.op == "edit_type" or s.op == "edit_end" then
    out.error = host_edit(s)
    out.stream = "[" .. table.concat(stream, ",") .. "]"
    out.focused = call(u.focused, u)
    out.editing = call(u.editing, u)
  end
  if s.op == "focus" then out.ret = call(u.focus, u, table.unpack(conv(s.args or {}), 1, s.n or 0)) end
  if s.op == "focused" then out.ret = call(u.focused, u) end
  if s.op == "layout" then
    out.result = layout_result(table.pack(pcall(u.layout, u, conv(s.rect), conv(s.spec))))
  end
  if s.op == "frame" or s.op == "begin" or s.op == "end" or s.op == "call" then
    out.stream = "[" .. table.concat(stream, ",") .. "]"
    if #host_errors > 0 then out.host_errors = "[" .. table.concat(host_errors, ",") .. "]" end
    out.stream_count = value(#stream)
    out.op_counts = next(counts) and value(counts) or "{}"
    out.focused_after = call(u.focused, u)
  elseif s.op == "event" or s.op == "focus" then
    out.focused = call(u.focused, u)
  end
  return object(out)
end

local done = false

function on_draw(ctx, g)
  if done then return end
  done = true
  G = g
  for _, name in ipairs({ "small", "medium", "large" }) do FONT_NAMES[ctx.fonts[name]] = name end
  HOST_LAYOUT = type(ctx.fonts.medium.layout) == "function"
  patch(g)
  ctx.input.edit = stub_edit
  if input.no_pattern then g.pattern = nil end
  if input.touch == false then ctx.input.pointer = false end
  local out = {}
  for i, s in ipairs(input.steps) do out[i] = run(i, s) end
  RESULT = table.concat(out, "\n")
end
