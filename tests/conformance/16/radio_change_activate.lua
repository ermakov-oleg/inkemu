local T = require("conformance")
local ui

-- five 40 px cells from y = 100 between two buttons
local function scene(g, o)
  o = o or {}
  ui:begin_frame(g)
  ui:button("top", { x = 20, y = 20, w = 200, h = 60, text = "top" })
  ui:radio("r", { x = 20, y = 100, w = 300, h = 200, items = { "Alpha", "Beta", "Gamma", "Delta", "Epsilon" },
    value = o.value or 2, enabled = o.enabled })
  ui:button("bottom", { x = 20, y = 400, w = 200, h = 60, text = "bottom" })
  ui:end_frame()
end

local function send(name, ev)
  T.call(name, function() return ui:handle_event(ev) end)
end

local function key(name, phase)
  return { type = "key", key = name, phase = phase or "down", time_ms = 0 }
end

local function pointer(phase, item, gesture)
  local y = 100 + (item - 1) * 40 + 10
  return { type = "pointer", phase = phase, x = 40, y = y, down_x = 40, down_y = y, gesture = gesture, time_ms = 0 }
end

function on_load(ctx)
  ui = ctx.ui:create()
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  scene(g)
  if done then return end
  done = true
  send("tap_other_item", pointer("up", 4, "tap"))
  send("tap_value_item", pointer("up", 2, "tap"))
  send("double_tap_other_item", pointer("up", 3, "double_tap"))
  send("hold_item", pointer("hold", 5))
  send("tap_plain_button", { type = "pointer", phase = "up", x = 30, y = 30, down_x = 30, down_y = 30,
    gesture = "tap", time_ms = 0 })
  -- the script took value 4; ok goes by the focused part
  ui:focus("r", 2)
  scene(g, { value = 4 })
  send("ok_down_part_2", key("ok"))
  send("ok_up_part_2", key("ok", "up"))
  send("down_to_part_3", key("down"))
  send("ok_down_part_3", key("ok"))
  send("ok_up_part_3", key("ok", "up"))
  ui:focus("r", 4)
  scene(g, { value = 4 })
  send("ok_down_value", key("ok"))
  send("ok_up_value", key("ok", "up"))
  -- the library keeps no value: without the script's update the old one is drawn and decides again
  send("tap_again_without_update", pointer("up", 3, "tap"))
  scene(g, { value = 4, enabled = false })
  send("tap_disabled", pointer("up", 3, "tap"))
  -- focus left the disabled radio for the nearest control
  send("ok_after_disabled", key("ok"))
end
