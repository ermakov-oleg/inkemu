-- ctx.ui (§16): the kept focus, checked against each frame at end_frame, `ui:focus` and `ui:focused`.
--
-- Focus, hits and the arrow keys work on targets: a focusable control is one target (`id`, no `part`, its box,
-- `enabled`, `nav`). A composite kind can list its parts as separate targets through `kind.targets(c)`.

-- the targets of a frame's controls, in declaration order
function targets_of(list)
  local out = {}
  for _, c in ipairs(list) do
    if c.kind.targets then
      for _, t in ipairs(c.kind.targets(c)) do
        out[#out + 1] = t
      end
    elseif c.kind.focusable then
      out[#out + 1] = c
    end
  end
  return out
end

-- the target `id`/`part` among `list`, or nil
function find_target(list, id, part)
  for _, t in ipairs(list) do
    if t.id == id and t.part == part then
      return t
    end
  end
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

-- in floats: the squares of far-apart integer centres overflow 32 bits
local function center_distance(a, b)
  local dx = (a.x * 2.0 + a.w) - (b.x * 2.0 + b.w)
  local dy = (a.y * 2.0 + a.h) - (b.y * 2.0 + b.h)
  return dx * dx + dy * dy
end

-- The kept focus stays on its target (a part set on a plain control is dropped), moves to the nearest available
-- target when that one is disabled, and falls on the first available target when the id is absent or names a
-- control that takes no focus.
function settle_focus(st, list)
  local first = first_available(list)
  local kept = find_target(list, st.focus, nil)
  if kept and kept.enabled then
    set_focus(st, kept.id, nil)
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

-- what painting needs to know about focus
function look_of(st, c)
  local focused = st.keys and c.kind.focusable and c.id == st.focus
  return { focused = focused == true, outline = st.focus_style == "outline" }
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
