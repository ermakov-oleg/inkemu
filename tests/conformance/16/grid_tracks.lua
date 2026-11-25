local T = require("conformance")
local B = require("layout_box")

local R = { x = 0, y = 0, w = 480, h = 800 }

local function grid(more, children)
  local s = { kind = "grid", children = children }
  for k, v in pairs(more) do s[k] = v end
  return s
end

-- `n` plain boxes
local function boxes(n)
  local out = {}
  for i = 1, n do out[i] = {} end
  return out
end

function on_load(ctx)
  local ui = ctx.ui:create()
  local m = ctx.fonts.medium
  local function run(name, rect, spec)
    T.value(name, B.run(ui, rect, spec))
  end
  -- integer columns are equal fr tracks; plain boxes are 1 px high in fit rows
  run("equal_columns", R, grid({ columns = 3 }, boxes(4)))
  run("auto_rows_gap_padding", R, grid({ columns = 3, auto_rows = 50, gap = 10, padding = 20 }, boxes(4)))
  -- { min }: as many 1fr columns as (inner + gap) // (min + gap), at least one
  run("min_columns", R, grid({ columns = { min = 140 }, gap = 12, auto_rows = 56 }, boxes(4)))
  run("min_wider_than_box", R, grid({ columns = { min = 500 }, auto_rows = 56 }, boxes(2)))
  -- fr shares what the other tracks and gaps leave; a percentage takes its share of the inner width
  run("fr_and_percent", R, grid({ columns = { "2fr", "1fr", "25%" }, column_gap = 8, row_gap = 4, auto_rows = 40 },
    boxes(3)))
  -- a fit track takes the widest child alone in it; fixed rows, an fr row and a fixed one share the height
  run("fixed_fr_fit", R, grid({ columns = { 96, "1fr", "fit" }, rows = { 64, "1fr", 72 }, gap = 12, padding = 16 }, {
    { column_span = 3 }, { column_span = 2 }, { w = 72 }, {}, {}, { w = 36 } }))
  -- a span over two fit tracks splits what it lacks evenly between them
  run("span_over_fit", R, grid({ columns = { "fit", "fit", "fit" }, auto_rows = 40 }, {
    { w = 48 }, { w = 96 }, { w = 36 }, { column_span = 2, w = 312 }, { w = 36 } }))
  -- a span that crosses an fr track does not size the tracks
  run("span_over_fr", R, grid({ columns = { "fit", "1fr" }, auto_rows = 40 }, {
    { w = 72 }, {}, { column_span = 2, w = 400 } }))
  -- the leftover goes into the gaps by justify_content and align_content; a span keeps the gap it crosses
  run("space_between_center", R, grid({ columns = { 100, 100 }, rows = { 50, 50 }, justify_content = "space-between",
    align_content = "center" }, boxes(4)))
  run("space_evenly_end", R, grid({ columns = { 100, 100, 100 }, rows = { 50 }, justify_content = "space-evenly",
    align_content = "end" }, { { column_span = 2 }, {} }))
  run("tracks_wider_than_grid", R, grid({ columns = { 300, 300 }, auto_rows = 50, justify_content = "center" },
    boxes(2)))
  -- content size: fit tracks from their children, fr tracks as many units as the largest needs
  local fit = { x = 10, y = 20, w = "fit", h = "fit" }
  run("fit_both_ways", fit, grid({ columns = { "fit", "1fr" }, gap = 6, padding = { 4, 8 } }, {
    { w = 36, h = 40 }, { w = 48, h = 40 }, { w = 60, h = 40 } }))
  run("fr_units", fit, grid({ columns = { "1fr", "2fr" } }, { { w = 60, h = 40 }, { w = 120, h = 40 } }))
  -- a { min } grid cannot fit its width; inside a fitted row it counts as 0 wide (then 1, its minimum)
  run("min_with_fit_width", { x = 0, y = 0, w = "fit", h = 100 }, grid({ columns = { min = 100 } }, boxes(1)))
  run("min_in_fitted_row", { x = 0, y = 0, w = "fit", h = "fit" }, { kind = "row", children = {
    grid({ columns = { min = 100 } }, boxes(2)), { w = 84, h = 40 } } })
  -- a row is as high as its children at their final widths: a label wraps at its column
  local text = "a long label that wraps at the column width of the grid cell"
  local b = ui:layout({ x = 0, y = 0, w = 300, h = 800 }, grid({ columns = 2 }, {
    { kind = "label", text = text }, { kind = "button", text = "b" } }))
  local _, lines = m:layout(text, 150, 32767)
  T.value("row_from_final_width", b[1].w == 150 and b[1].h == lines and b[2].h == lines)
  -- a button left at its content size in a cell, centred, and one stretched
  local c = ui:layout({ x = 0, y = 0, w = 200, h = 100 }, grid({ columns = 1, rows = { 100 }, justify_items = "center",
    align_items = "end" }, { { kind = "button", text = "one" } }))
  local bw, bh = m:measure("one") + 24, m.line_height + 16
  T.value("content_in_cell", c[1].x == (200 - bw) // 2 and c[1].y == 100 - bh and c[1].w == bw and c[1].h == bh)
end
