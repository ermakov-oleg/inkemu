local T = require("conformance")
local B = require("layout_box")

-- the widths of the children of a 100 × 50 row
local function widths(ui, children, more)
  local spec = { kind = "row", children = children }
  for k, v in pairs(more or {}) do spec[k] = v end
  local b, err = ui:layout({ x = 0, y = 0, w = 100, h = 50 }, spec)
  if b == nil then return "nil," .. tostring(err) end
  return B.list(b)
end

function on_load(ctx)
  local ui = ctx.ui:create()
  local g1 = { grow = 1 }
  T.value("grow_1_1_1", widths(ui, { g1, { grow = 1 }, { grow = 1 } }))
  T.value("grow_1_2_3", widths(ui, { { grow = 1 }, { grow = 2 }, { grow = 3 } }))
  T.value("grow_gap_10", widths(ui, { { grow = 1 }, { grow = 1 }, { grow = 1 } }, { gap = 10 }))
  T.value("grow_max_w", widths(ui, { { grow = 1, max_w = 20 }, { grow = 1 }, { grow = 1 } }))
  T.value("grow_min_w", widths(ui, { { grow = 1, min_w = 60 }, { grow = 1 }, { grow = 1 } }))
  T.value("grow_on_a_base", widths(ui, { { w = 30, grow = 1 }, { grow = 1 } }))
  T.value("shrink_by_base", widths(ui, { { w = 60 }, { w = 60 }, { w = 30 } }))
  T.value("shrink_0", widths(ui, { { w = 60, shrink = 0 }, { w = 60 }, { w = 30 } }))
  T.value("shrink_min_w", widths(ui, { { w = 60, min_w = 55 }, { w = 60 }, { w = 30 } }))
  T.value("shrink_2", widths(ui, { { w = 70 }, { w = 70, shrink = 2 } }))
  T.value("no_fit_at_min", widths(ui, { { w = 80, min_w = 80 }, { w = 80, min_w = 80 } }))
  -- the first child stops at its min; the loop splits what is left between the others again
  T.value("shrink_to_min_then_again", widths(ui, { { w = 60, min_w = 54 }, { w = 60 }, { w = 40, shrink = 3 } }))
  local b = ui:layout({ x = 0, y = 0, w = 200, h = 50 }, { kind = "row", padding = 10, children = {
    { w = "50%" }, { w = "25%" }, { grow = 1 } } })
  T.value("percent_of_inner", B.text(b))
end
