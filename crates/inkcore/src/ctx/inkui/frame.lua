-- ctx.ui (§16): the frame. Controls are checked and registered when declared and painted at end_frame, in
-- declaration order, so whatever the script draws between begin_frame and end_frame ends up below them.

MAX_CONTROLS = 128

-- control kinds by name, filled by the control files: { check = function(c, o) -> ok[, unsupported message],
-- paint = function(g, c, look), focusable = bool, cost = controls counted against MAX_CONTROLS: a number, 1 when
-- nil, or a function of the control record }; composite.lua adds the fields of composite kinds
kinds = {}

-- a canvas is the frame's g (or a layer's): host objects that cannot be told apart from Lua, so look at the methods
function is_canvas(g)
  return type(g) == "table" and type(g.rect) == "function" and type(g.text_box) == "function"
end

methods.begin_frame = function(self, g)
  local st = state_of(self, "begin_frame")
  if st.frame then
    return nil, "busy"
  end
  if not is_canvas(g) then
    return nil, INVALID
  end
  st.frame = { g = g, list = {}, ids = {}, count = 0 }
end

methods.end_frame = function(self)
  local st = state_of(self, "end_frame")
  local f = st.frame
  if f == nil then
    return nil, INVALID
  end
  -- closed before painting: an error raised by a region's draw still ends the frame
  st.frame = nil
  st.targets = targets_of(f.list)
  settle_focus(st, st.targets)
  for _, c in ipairs(f.list) do
    if c.kind.paint then
      c.kind.paint(f.g, c, look_of(st, c))
    end
  end
end

-- how many of a frame's MAX_CONTROLS the control record `c` takes
function cost_of(c)
  local cost = c.kind.cost
  if type(cost) == "function" then
    return cost(c)
  end
  return cost or 1
end

-- Reserves `n` of the frame's controls; false once the frame would hold more than MAX_CONTROLS.
function reserve(f, n)
  if f.count + n > MAX_CONTROLS then
    return false
  end
  f.count = f.count + n
  return true
end

-- A control declared at top level by `ui:<name>(id, o)`: checked, counted, then registered for painting. Returns
-- nothing, or nil and the error code. `level` is error()'s level of the app's call; callers must not tail-call this.
function declare(st, name, id, o, level)
  local f = st.frame
  local kind = kinds[name]
  if f == nil or type(id) ~= "string" or id == "" or f.ids[id] or type(o) ~= "table" then
    return nil, INVALID
  end
  if not (is_integral(o.x) and is_integral(o.y) and is_side(o.w) and is_side(o.h)) then
    return nil, INVALID
  end
  local c = { kind = kind, id = id, x = o.x, y = o.y, w = o.w, h = o.h }
  local ok, unsupported = kind.check(c, o)
  if unsupported then
    error(unsupported, level)
  end
  if not ok then
    return nil, INVALID
  end
  if not reserve(f, cost_of(c)) then
    return nil, "limit_exceeded"
  end
  f.ids[id] = true
  f.list[#f.list + 1] = c
end
