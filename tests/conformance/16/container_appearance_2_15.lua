local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  ui:column("c", { x = 0, y = 0, w = 100, h = 60, background = 5, radius = 6, border = true, children = { { kind = "spacer" } } })
  ui:column("d", { x = 110, y = 0, w = 100, h = 100, divider = { color = 6, line_width = 2 }, children = {
    { kind = "region", id = "a", h = 40 }, { kind = "region", id = "b", h = 40 } } })
  local log = L.start(g)
  ui:end_frame()
  T.value("calls", L.text(log) == table.concat({
    L.call("rect", 0, 0, 100, 60, { color = 5, fill = true, radius = 6 }),
    L.call("rect", 0, 0, 100, 60, { color = 15, line_width = 1, radius = 6 }),
    L.call("rect", 110, 39, 100, 2, { color = 6, fill = true }),
  }, " "))
end
