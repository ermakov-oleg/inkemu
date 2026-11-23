-- ctx.ui (§16): the arrow keys. A package for API 2.13 or later moves focus by position, an older one in
-- declaration order. Both work on the targets of the last closed frame.

-- per direction: the main axis (position, size), the cross axis, and +1 when moving toward larger coordinates
local AXES = {
  left = { "x", "w", "y", "h", -1 },
  right = { "x", "w", "y", "h", 1 },
  up = { "y", "h", "x", "w", -1 },
  down = { "y", "h", "x", "w", 1 },
}

local FORWARD = { down = true, right = true }

local function moved(st, t)
  set_focus(st, t.id, t.part)
  return event_result(t.id, "focus", t.part)
end

-- `nav[dir]` of the current target when it names an available plain control
local function override(list, cur, dir)
  local nav = cur.nav
  local id = nav and nav[dir]
  if id == nil then
    return nil
  end
  local t = find_target(list, id, nil)
  if t and t.enabled then
    return t
  end
end

-- (overlaps on the cross axis, gap on the main axis, distance of the centres on the cross axis), or nil when `t`
-- is not on the `dir` side of `cur`. Centres are doubled to stay in integers.
local function rank(cur, t, dir)
  local p, z, q, k, s = table.unpack(AXES[dir])
  local center, cur_center = 2 * t[p] + t[z], 2 * cur[p] + cur[z]
  local gap
  if s > 0 then
    if not (center > cur_center and t[p] >= cur[p]) then
      return nil
    end
    gap = t[p] - (cur[p] + cur[z])
  else
    if not (center < cur_center and t[p] + t[z] <= cur[p] + cur[z]) then
      return nil
    end
    gap = cur[p] - (t[p] + t[z])
  end
  local overlap = t[q] < cur[q] + cur[k] and cur[q] < t[q] + t[k]
  local cross = math.abs((2 * t[q] + t[k]) - (2 * cur[q] + cur[k]))
  return overlap, math.max(0, gap), cross
end

local function better(o, g, c, bo, bg, bc)
  if o ~= bo then
    return o
  end
  if g ~= bg then
    return g < bg
  end
  return c < bc
end

local function by_position(list, cur, dir)
  local target = override(list, cur, dir)
  if target then
    return target
  end
  local best, bo, bg, bc = nil, nil, nil, nil
  for _, t in ipairs(list) do
    if t.enabled and t ~= cur then
      local o, g, c = rank(cur, t, dir)
      if o ~= nil and (best == nil or better(o, g, c, bo, bg, bc)) then
        best, bo, bg, bc = t, o, g, c
      end
    end
  end
  return best
end

-- the next or previous available target; at either end the current one again
local function by_order(list, cur, dir)
  local at
  for i, t in ipairs(list) do
    if t == cur then
      at = i
    end
  end
  local step = FORWARD[dir] and 1 or -1
  local i = at + step
  while list[i] do
    if list[i].enabled then
      return list[i]
    end
    i = i + step
  end
  return cur
end

-- `id, "focus"[, part]` for an arrow key `dir`, or `nil, nil` when focus stays
function navigate(st, dir)
  local list = st.targets
  local cur = find_target(list, st.focus, st.part)
  if cur == nil then
    local first = first_available(list)
    if first == nil then
      return nil, nil
    end
    return moved(st, first)
  end
  local t
  if api_at_least(13) then
    t = by_position(list, cur, dir)
  else
    t = by_order(list, cur, dir)
  end
  if t == nil then
    return nil, nil
  end
  return moved(st, t)
end
