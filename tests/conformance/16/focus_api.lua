local T = require("conformance")
local ui

local function frame(g)
  ui:begin_frame(g)
  ui:button("A", { x = 40, y = 300, w = 100, h = 60, text = "A" })
  ui:button("B", { x = 190, y = 300, w = 100, h = 60, text = "B" })
  ui:button("C", { x = 340, y = 300, w = 100, h = 60, text = "C" })
  ui:end_frame()
end

local function key(name, phase)
  return ui:handle_event({ type = "key", key = name, phase = phase or "down", time_ms = 0 })
end


function on_load(ctx)
  ui = ctx.ui:create()
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  frame(g)
  if done then return end
  done = true
  T.call("focus", function() return ui:focus("C") end)
  T.call("focused_at_once", function() return ui:focused() end)
  T.call("ok_down", function() return key("ok") end)
  T.call("ok_up", function() return key("ok", "up") end)
  T.call("arrow_from_kept", function() return key("left") end)
  ui:focus("ghost")
  T.call("unknown_until_end_frame", function() return ui:focused() end)
  T.call("ok_on_unknown", function() return key("ok") end)
  T.call("arrow_from_unknown", function() return key("right") end)
  ui:focus("ghost")
  frame(g)
  T.call("unknown_at_end_frame", function() return ui:focused() end)
  ui:begin_frame(g)
  ui:focus("B")
  ui:button("A", { x = 40, y = 300, w = 100, h = 60, text = "A" })
  ui:button("B", { x = 190, y = 300, w = 100, h = 60, text = "B" })
  ui:end_frame()
  T.call("inside_a_frame", function() return ui:focused() end)
  T.call("empty_id", function() return ui:focus("") end)
  T.value("number_id_raises", not pcall(ui.focus, ui, 5))
  T.value("nil_id_raises", not pcall(ui.focus, ui, nil))
  T.value("no_id_raises", not pcall(ui.focus, ui))
  T.call("part_zero", function() return ui:focus("B", 0) end)
  T.call("part_negative", function() return ui:focus("B", -1) end)
  T.call("part_fraction", function() return ui:focus("B", 1.5) end)
  T.call("part_string", function() return ui:focus("B", "2") end)
  T.call("after_refusals", function() return ui:focused() end)
  T.call("part_whole_float", function() return ui:focus("B", 2.0) end)
  T.call("part_kept", function() return ui:focused() end)
  T.value("part_is_integer", math.type(select(2, ui:focused())))
  frame(g)
  T.call("part_dropped_on_a_plain_control", function() return ui:focused() end)
end
