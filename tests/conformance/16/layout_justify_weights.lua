local T = require("conformance")
local B = require("layout_box")

local function three()
  return { { w = 20, shrink = 0 }, { w = 20, shrink = 0 }, { w = 20, shrink = 0 } }
end

-- the x of three 20 px children in a row `w` wide
local function xs(ui, w, mode, children)
  local b = ui:layout({ x = 0, y = 0, w = w, h = 30 }, { kind = "row", justify_content = mode, children = children or three() })
  return B.list(b, "x") .. (b.overflow and ("!" .. b.overflow) or "")
end

function on_load(ctx)
  local ui = ctx.ui:create()
  -- 101 px leave 41 free
  for _, mode in ipairs({ "start", "end", "center", "space-between", "space-around", "space-evenly" }) do
    T.value("free_41." .. mode, xs(ui, 101, mode))
  end
  -- 50 px leave -10: the space modes fall back to start, center splits the overflow
  T.value("free_minus_10.center", xs(ui, 50, "center"))
  T.value("free_minus_10.space-between", xs(ui, 50, "space-between"))
  T.value("free_minus_10.end", xs(ui, 50, "end"))
  T.value("one_child.space-between", xs(ui, 101, "space-between", { { w = 20 } }))
  T.value("one_child.space-around", xs(ui, 101, "space-around", { { w = 21 } }))
  local b = ui:layout({ x = 0, y = 0, w = 30, h = 101 }, { kind = "column", justify_content = "space-around",
    children = { { h = 20 }, { h = 20 }, { h = 20 } } })
  T.value("column.space-around", B.list(b, "y"))
end
