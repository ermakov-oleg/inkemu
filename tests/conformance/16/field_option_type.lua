local T = require("conformance")
local ui

local function tap_at(x, y)
  return { type = "pointer", phase = "up", gesture = "tap", x = x, y = y, down_x = x, down_y = y, time_ms = 0 }
end

function on_load(ctx)
  ui = ctx.ui:create()
end

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  ui:field("typed", { x = 24, y = 40, w = 432, h = 56, multiline = "yes" })
  ui:field("ranged", { x = 24, y = 110, w = 432, h = 56, max_chars = 0 })
  ui:end_frame()
  T.value("typed_raises", not pcall(ui.handle_event, ui, tap_at(100, 60)))
  T.value("typed_editing", tostring(ui:editing()))
  T.call("ranged", function() return ui:handle_event(tap_at(100, 130)) end)
  T.value("ranged_editing", tostring(ui:editing()))
end
