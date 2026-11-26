local T = require("conformance")
local B = require("layout_box")
local ui

local function tap(name, x, y)
  T.call(name, function()
    return ui:handle_event({ type = "pointer", phase = "up", gesture = "tap", x = x, y = y, down_x = x, down_y = y,
      time_ms = 0 })
  end)
end

function on_load(ctx)
  ui = ctx.ui:create()
  local m = ctx.fonts.medium
  -- every child in the inner box by justify_self and align_self; percentages of the inner box
  local b = ui:layout({ x = 40, y = 120, w = 400, h = 300 }, { kind = "stack", padding = 12, children = {
    { w = 100, h = 50, justify_self = "center", align_self = "end" },
    { w = 36, h = 24, justify_self = "end", align_self = "start" },
    {},
    { w = "50%", h = "25%", justify_self = "end", align_self = "end" },
    { w = 60, aspect_ratio = 2, justify_self = "start", align_self = "center" },
  } })
  T.value("place", B.text(b))
  -- a fitted label at the top right corner
  local tag = ui:layout({ x = 40, y = 120, w = 400, h = 300 }, { kind = "stack", padding = 12, children = {
    { kind = "label", text = "New", w = "fit", h = "fit", justify_self = "end", align_self = "start" } } })[1]
  local _, lh = m:layout("New", m:measure("New"), 32767)
  T.value("fit_label", tag.x == 52 + 376 - m:measure("New") and tag.y == 132 and tag.h == lh)
  -- a button left at its content size, centred both ways
  local c = ui:layout({ x = 0, y = 0, w = 300, h = 200 }, { kind = "stack", justify_items = "center",
    align_items = "center", children = { { kind = "button", text = "center" } } })
  local bw, bh = m:measure("center") + 24, m.line_height + 16
  T.value("centred_button", c[1].x == (300 - bw) // 2 and c[1].y == (200 - bh) // 2 and c[1].w == bw
    and c[1].h == bh)
  -- content size: padding and the largest child each way; stretched children then fill the inner box
  T.value("fit", B.run(ui, { x = 0, y = 0, w = "fit", h = "fit" }, { kind = "stack", padding = { 5, 6 }, children = {
    { w = 84, h = 40 }, { w = 228, h = 24 }, { w = 30, h = 60 }, {} } }))
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  ui:stack("sk", { x = 10, y = 300, w = 300, h = 200, padding = 10, border = true, children = {
    { kind = "button", id = "under", text = "under" },
    { kind = "button", id = "over", text = "over", w = 100, h = 50, justify_self = "end", align_self = "end" },
  } })
  ui:end_frame()
  if done then return end
  done = true
  -- the later child is drawn over the earlier one and takes the hit where they overlap
  tap("overlap", 270, 480)
  tap("earlier_elsewhere", 100, 320)
end
