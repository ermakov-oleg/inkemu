local T = require("conformance")
local L = require("canvas_log")
local ui

-- top (with an optional nav), five 40 px radio cells from y = 100, and a button below
local function scene(g, o)
  o = o or {}
  ui:begin_frame(g)
  ui:button("top", { x = 20, y = 20, w = 200, h = 60, text = "top", nav = o.nav })
  ui:radio("r", { x = 20, y = 100, w = 300, h = 200, items = { "Alpha", "Beta", "Gamma", "Delta", "Epsilon" },
    value = o.value, enabled = o.enabled, nav = o.radio_nav })
  ui:button("bottom", { x = 20, y = 400, w = 200, h = 60, text = "bottom" })
  ui:end_frame()
end

local function key(name)
  return ui:handle_event({ type = "key", key = name, phase = "down", time_ms = 0 })
end

function on_load(ctx)
  ui = ctx.ui:create()
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then
    scene(g, { value = 4 })
    return
  end
  done = true
  scene(g, { value = 4 })
  T.call("first_frame", function() return ui:focused() end)
  T.call("focus_without_part", function() return ui:focus("r") end)
  T.call("before_end_frame", function() return ui:focused() end)
  scene(g, { value = 4 })
  T.call("default_is_value", function() return ui:focused() end)
  ui:focus("r", 1)
  scene(g, { value = 4 })
  T.call("part_kept", function() return ui:focused() end)
  ui:focus("r", 9)
  scene(g, { value = 4 })
  T.call("missing_part_to_default", function() return ui:focused() end)
  ui:focus("r")
  scene(g)
  T.call("no_value_default_first", function() return ui:focused() end)
  T.call("part_zero", function() return ui:focus("r", 0) end)
  T.call("part_fraction", function() return ui:focus("r", 1.5) end)
  T.call("part_string", function() return ui:focus("r", "2") end)
  -- arrows treat each part as its own control
  ui:focus("top")
  scene(g, { value = 4 })
  T.call("down_into_first_part", function() return key("down") end)
  T.call("down_part_2", function() return key("down") end)
  for _ = 1, 3 do key("down") end
  T.call("down_out_of_last_part", function() return key("down") end)
  T.call("up_into_last_part", function() return key("up") end)
  T.call("right_from_part", function() return key("right") end)
  -- a nav entry naming the radio goes to its default part; a nav on the radio itself is ignored
  ui:focus("top")
  scene(g, { value = 4, nav = { down = "r" } })
  T.call("nav_to_radio", function() return key("down") end)
  ui:focus("r", 1)
  scene(g, { value = 4, radio_nav = { up = "bottom" } })
  T.call("nav_on_radio_ignored", function() return key("up") end)
  -- the focus frame goes around the focused cell, right after that cell's text
  ui:focus("r", 3)
  scene(g, { value = 4 })
  ui:begin_frame(g)
  ui:radio("r", { x = 20, y = 100, w = 300, h = 200, items = { "Alpha", "Beta", "Gamma", "Delta", "Epsilon" },
    value = 4 })
  local log = L.start(g)
  ui:end_frame()
  local ring = L.call("rect", 17, 177, 306, 46, { line_width = 2, radius = 0 })
  local at
  for i, c in ipairs(log) do
    if c == ring then at = i end
  end
  T.value("ring_after_cell_3", at ~= nil and string.find(log[at - 1], "Gamma", 1, true) ~= nil
    and string.sub(log[at + 1], 1, 7) == "circle(")
  -- a disabled radio hands focus on to the control whose centre is nearest the focused part
  ui:focus("r", 5)
  scene(g, { value = 4, enabled = false })
  T.call("disabled_to_nearest", function() return ui:focused() end)
end
