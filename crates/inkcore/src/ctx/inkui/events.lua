-- ctx.ui (§16): `ui:handle_event`. Keys act on the kept focus, pointer events on the target under the press point;
-- both look at the last closed frame.

local ARROWS = { up = true, down = true, left = true, right = true }
local TAPS = { tap = true, double_tap = true }

-- `id, action`, with the part as a third value only for a part of a composite control
function result(id, action, part)
  if part ~= nil then
    return id, action, part
  end
  return id, action
end

-- where the finger went down, else where the event happened, or nil
local function press_point(ev)
  if type(ev.down_x) == "number" and type(ev.down_y) == "number" then
    return ev.down_x, ev.down_y
  end
  if type(ev.x) == "number" and type(ev.y) == "number" then
    return ev.x, ev.y
  end
end

-- the last added target containing the point; a disabled one takes the hit and passes nothing on
local function hit(list, px, py)
  for i = #list, 1, -1 do
    local t = list[i]
    if px >= t.x and px < t.x + t.w and py >= t.y and py < t.y + t.h then
      if t.enabled then
        return t
      end
      return nil
    end
  end
end

local function key(st, ev)
  local down = ev.phase == "down"
  if ev.key == "ok" then
    local t = find_target(st.targets, st.focus, st.part)
    local pressed = st.pressed
    st.pressed = nil
    if st.focus == nil or (t and not t.enabled) then
      return nil, nil
    end
    if down then
      st.pressed = true
      return result(st.focus, "press", st.part)
    end
    if pressed then
      return result(st.focus, "activate", st.part)
    end
    return nil, nil
  end
  if down and ARROWS[ev.key] then
    return navigate(st, ev.key)
  end
  return nil, nil
end

local function pointer(st, ev)
  local tap = ev.phase == "up" and TAPS[ev.gesture]
  if not (tap or ev.phase == "hold") then
    return nil, nil
  end
  local px, py = press_point(ev)
  local t = px and hit(st.targets, px, py)
  if not t then
    return nil, nil
  end
  if not tap then
    return result(t.id, "long_press", t.part)
  end
  set_focus(st, t.id, t.part)
  return result(t.id, "activate", t.part)
end

methods.handle_event = function(self, ev)
  local st = state_of(self, "handle_event")
  if type(ev) ~= "table" then
    return nil, INVALID
  end
  if ev.type == "key" then
    if ev.phase ~= "down" and ev.phase ~= "up" then
      return nil, INVALID
    end
    st.keys = true
    return key(st, ev)
  end
  if ev.type == "pointer" then
    st.keys = false
    return pointer(st, ev)
  end
  return nil, INVALID
end
