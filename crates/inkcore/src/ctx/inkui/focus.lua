-- ctx.ui (§16): the kept focus, checked against each frame at end_frame, `ui:focus` and `ui:focused`.
--
-- Focus, hits and the arrow keys work on targets: a focusable control is one target (`id`, no `part`, its box,
-- `enabled`, `nav`). A composite kind lists its parts as separate targets through `kind.targets(c)`, and names the
-- part that focus on the whole control takes through `kind.default_part(c)` (composite.lua).

-- the targets of a frame's controls in declaration order, and the controls that own them by id
function targets_of(list)
  local out, owners = {}, {}
  for _, c in ipairs(list) do
    if c.kind.targets then
      owners[c.id] = c
      for _, t in ipairs(c.kind.targets(c)) do
        out[#out + 1] = t
      end
    elseif c.kind.focusable then
      owners[c.id] = c
      out[#out + 1] = c
    end
  end
  return out, owners
end

-- the target `id`/`part` among `list`, or nil
function find_target(list, id, part)
  for _, t in ipairs(list) do
    if t.id == id and t.part == part then
      return t
    end
  end
end

-- The target that focus on `id` and `part` stands for among `list`, whose controls `owners` holds: a plain control
-- whatever the part, a composite's part, or its default part when `part` is nil or not in the frame. Nil when `id`
-- owns no target.
function resolve(list, owners, id, part)
  local c = id ~= nil and owners[id]
  if not c then
    return nil
  end
  if c.kind.targets == nil then
    return c
  end
  return (part and find_target(list, id, part)) or find_target(list, id, c.kind.default_part(c))
end

function first_available(list)
  for _, t in ipairs(list) do
    if t.enabled then
      return t
    end
  end
end

-- Moves the kept focus; an `ok` down no longer pairs with an `ok` up once focus has moved.
function set_focus(st, id, part)
  if id ~= st.focus or part ~= st.part then
    st.pressed = nil
  end
  st.focus, st.part = id, part
end

-- what handle_event returns: `id, action`, plus the part number when the target is one of a composite's parts
function event_result(id, action, part)
  if part ~= nil then
    return id, action, part
  end
  return id, action
end

-- in floats: the squares of far-apart integer centres overflow 32 bits
local function center_distance(a, b)
  local dx = (a.x * 2.0 + a.w) - (b.x * 2.0 + b.w)
  local dy = (a.y * 2.0 + a.h) - (b.y * 2.0 + b.h)
  return dx * dx + dy * dy
end

-- At end_frame: the kept focus stays on the target it stands for (see `resolve`), moves to the nearest available
-- target when that one is disabled, and goes to the first available target when the id is absent or names a
-- control that takes no focus.
function settle_focus(st)
  local list = st.targets
  local first = first_available(list)
  local kept = resolve(list, st.owners, st.focus, st.part)
  if kept and kept.enabled then
    set_focus(st, kept.id, kept.part)
    return
  end
  if kept == nil or first == nil then
    if first then
      set_focus(st, first.id, first.part)
    else
      set_focus(st, nil, nil)
    end
    return
  end
  local best, best_d = nil, nil
  for _, t in ipairs(list) do
    if t.enabled then
      local d = center_distance(t, kept)
      if best == nil or d < best_d then
        best, best_d = t, d
      end
    end
  end
  set_focus(st, best.id, best.part)
end

-- what painting needs to know about focus: whether it is drawn on control `c`, and on which of its parts
function look_of(st, c)
  local focused = st.keys and c.kind.focusable and c.id == st.focus
  return { focused = focused == true, part = focused and st.part or nil, outline = st.focus_style == "outline" }
end

methods.focused = function(self)
  local st = state_of(self, "focused")
  if st.part ~= nil then
    return st.focus, st.part
  end
  return st.focus
end

methods.focus = function(self, id, part)
  local st = state_of(self, "focus")
  if type(id) ~= "string" then
    error("ui:focus: id must be a string", 2)
  end
  if id == "" then
    return nil, INVALID
  end
  if part ~= nil then
    part = is_integral(part) and part >= 1 and math.tointeger(part)
    if not part then
      return nil, INVALID
    end
  end
  set_focus(st, id, part)
end
