local T = require("conformance")
local B = require("layout_box")

function on_load(ctx)
  local ui = ctx.ui:create()
  local R = { x = 0, y = 0, w = 200, h = 101 }
  for _, mode in ipairs({ "start", "center", "end", "stretch" }) do
    -- the second child has no height and no content, so it is stretched whatever the alignment
    T.value("align_items." .. mode, B.run(ui, R, { kind = "row", align_items = mode,
      children = { { w = 50, h = 40 }, { w = 50 } } }))
  end
  T.value("align_self", B.run(ui, R, { kind = "row", align_items = "center",
    children = { { w = 50, h = 40, align_self = "end" }, { w = 50, h = 40 } } }))
  T.value("stretch_to_max_h", B.run(ui, R, { kind = "row", children = { { w = 50, max_h = 60 } } }))
  -- a cross-axis bound leaves the main size alone
  T.value("max_h_grown", B.run(ui, R, { kind = "row", children = { { grow = 1, max_h = 60 } } }))
  T.value("max_h_sized", B.run(ui, R, { kind = "row", children = { { w = 150, max_h = 20 } } }))
  T.value("max_w_grown", B.run(ui, R, { kind = "column", children = { { grow = 1, max_w = 60 } } }))
  -- three 40 px children in 100 px with gap 10 make two lines; align_content stretch shares the rest
  T.value("wrap_stretch", B.run(ui, { x = 0, y = 0, w = 100, h = 100 }, { kind = "row", wrap = true, gap = 10,
    children = { { w = 40, h = 20 }, { w = 40, h = 20 }, { w = 40, h = 20 } } }))
  T.value("wrap_center", B.run(ui, { x = 0, y = 0, w = 300, h = 200 }, { kind = "row", wrap = true, gap = 10,
    align_content = "center", children = { { w = 200, h = 20 }, { w = 200, h = 30 } } }))
  -- aspect_ratio: width first, then height; a derived size is not stretched
  T.value("aspect_row", B.run(ui, { x = 0, y = 0, w = 300, h = 200 }, { kind = "row",
    children = { { h = 50, aspect_ratio = 2 }, { w = 100, aspect_ratio = 4 }, { grow = 1 } } }))
  T.value("aspect_column", B.run(ui, { x = 0, y = 0, w = 300, h = 200 }, { kind = "column",
    children = { { aspect_ratio = 3 }, { w = 60, aspect_ratio = 2 } } }))
  -- baseline: a large label and a medium button, both sized from their content
  local large, medium = ctx.fonts.large, ctx.fonts.medium
  local b = ui:layout({ x = 0, y = 0, w = 400, h = 100 }, { kind = "row", align_items = "baseline", children = {
    { kind = "label", text = "Title", font = large, w = "fit", h = "fit" },
    { kind = "button", text = "Go", w = "fit", h = "fit" } } })
  local label_base = large.ascent
  local button_base = (medium.line_height + 16 - medium.line_height) // 2 + medium.ascent
  local top = math.max(label_base, button_base)
  T.value("baseline", b[1].y == top - label_base and b[2].y == top - button_base
    and b[1].h == large.line_height and b[2].h == medium.line_height + 16)
end
