local T = require("conformance")
local B = require("layout_box")

function on_load(ctx)
  local ui = ctx.ui:create()
  local function run(name, rect, spec)
    T.value(name, B.run(ui, rect, spec))
  end
  run("none", { x = 0, y = 0, w = 100, h = 100 }, { kind = "column", children = { { h = 60 }, { h = 40 } } })
  run("past_the_end", { x = 0, y = 0, w = 100, h = 100 }, { kind = "column",
    children = { { h = 60, min_h = 60 }, { h = 60, min_h = 60 } } })
  run("before_the_start", { x = 0, y = 0, w = 50, h = 30 }, { kind = "row", justify_content = "center",
    children = { { w = 20, shrink = 0 }, { w = 20, shrink = 0 }, { w = 20, shrink = 0 } } })
  -- padding lies outside the inner box: a child in it overflows
  run("into_padding", { x = 0, y = 0, w = 100, h = 50 }, { kind = "row", padding = 10,
    children = { { w = 70, shrink = 0 }, { w = 20, shrink = 0 } } })
  -- the position counts skipped children
  run("after_a_skipped_child", { x = 0, y = 0, w = 100, h = 100 }, { kind = "column",
    children = { false, { h = 60, min_h = 60 }, { h = 60, min_h = 60 } } })
  run("cross_axis", { x = 0, y = 0, w = 100, h = 30 }, { kind = "row", children = { { w = 10, h = 40 } } })
  run("nested_only", { x = 0, y = 0, w = 200, h = 200 }, { kind = "column", children = {
    { h = 50 }, { kind = "row", h = 50, children = { { w = 150, min_w = 150 }, { w = 150, min_w = 150 } } } } })
end
