-- ctx.ui (§16): the kept focus, checked against each frame at end_frame, and how a control learns it is focused.

local function available(c)
  return c.kind.focusable and c.enabled
end

local function center_distance(a, b)
  local dx = (a.x * 2 + a.w) - (b.x * 2 + b.w)
  local dy = (a.y * 2 + a.h) - (b.y * 2 + b.h)
  return dx * dx + dy * dy
end

-- The kept focus stays on its control, moves to the nearest available one when that control is disabled, and
-- falls on the first available control when the id is absent (or names a control that takes no focus).
function settle_focus(st, list)
  local first, kept = nil, nil
  for _, c in ipairs(list) do
    if first == nil and available(c) then
      first = c
    end
    if c.kind.focusable and c.id == st.focus then
      kept = c
    end
  end
  if kept and kept.enabled then
    return
  end
  if kept == nil or first == nil then
    st.focus = first and first.id
    return
  end
  local best, best_d = nil, nil
  for _, c in ipairs(list) do
    if available(c) then
      local d = center_distance(c, kept)
      if best == nil or d < best_d then
        best, best_d = c, d
      end
    end
  end
  st.focus = best.id
end

-- what painting needs to know about focus
function look_of(st, c)
  local focused = st.keys and c.kind.focusable and c.id == st.focus
  return { focused = focused == true, outline = st.focus_style == "outline" }
end

methods.focused = function(self)
  return state_of(self, "focused").focus
end
