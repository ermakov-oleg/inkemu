local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  ui:button("e", { x = 20, y = 20, w = 200, h = 56, text = "E", emphasis = true })
  local log = L.start(g)
  ui:end_frame()
  T.value("emphasis.filled", log[1] == L.call("rect", 20, 20, 200, 56, { fill = true, radius = 0 }))
  T.value("calls", L.text(log))
end
