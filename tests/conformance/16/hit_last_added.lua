local T = require("conformance")
local ui

local function tap(name, x, y)
  T.call(name, function()
    return ui:handle_event({ type = "pointer", phase = "up", gesture = "tap", x = x, y = y, down_x = x, down_y = y,
      time_ms = 0 })
  end)
end

function on_load(ctx)
  ui = ctx.ui:create()
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  ui:button("under", { x = 20, y = 20, w = 200, h = 100, text = "U" })
  ui:button("over", { x = 120, y = 60, w = 200, h = 100, text = "O" })
  ui:region("reg", { x = 20, y = 200, w = 200, h = 60 })
  ui:button("covered", { x = 20, y = 300, w = 200, h = 60, text = "C" })
  ui:button("lid", { x = 20, y = 300, w = 100, h = 60, text = "L", enabled = false })
  ui:region("lid2", { x = 120, y = 400, w = 100, h = 60, enabled = false })
  ui:button("free", { x = 20, y = 400, w = 200, h = 60, text = "F" })
  ui:label("text", { x = 0, y = 0, w = 480, h = 500, text = "" })
  ui:end_frame()
  if done then return end
  done = true
  tap("under_only", 30, 30)
  tap("overlap", 150, 80)
  tap("over_only", 300, 150)
  tap("region", 100, 230)
  tap("disabled_on_top", 50, 330)
  tap("beside_disabled", 150, 330)
  tap("declared_after_disabled", 150, 430)
  tap("right_edge_is_outside", 220, 30)
  tap("left_edge_is_inside", 20, 30)
  tap("nothing", 400, 700)
  T.value("focused", ui:focused())
end
