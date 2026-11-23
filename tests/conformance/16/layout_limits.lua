local T = require("conformance")
local ui

-- `n` containers in all, the outermost included; the innermost holds `leaf` (or nothing)
local function chain(n, leaf)
  local kids = leaf and { leaf } or {}
  for _ = n, 2, -1 do
    kids = { { kind = "column", children = kids } }
  end
  return kids
end

local function spacers(n)
  local t = {}
  for i = 1, n do t[i] = { kind = "spacer", h = 1 } end
  return t
end

local BUTTON = { kind = "button", id = "leaf", text = "d", w = 40, h = 30 }
local SPACER = { kind = "spacer", h = 1 }

function on_load(ctx)
  ui = ctx.ui:create()
  local R = { x = 0, y = 0, w = 200, h = 200 }
  local function layout(children)
    local r = table.pack(ui:layout(R, { kind = "column", children = children }))
    return type(r[1]) == "table" and "table" or tostring(r[2])
  end
  T.value("layout.16_levels_and_a_control", layout(chain(16, { id = "leaf", w = 40, h = 30 })))
  T.value("layout.17_levels", layout(chain(17)))
  T.value("layout.1024_children", layout(spacers(1024)))
  T.value("layout.1025_children", layout(spacers(1025)))
  -- children are counted over the whole tree
  T.value("layout.2_x_600_children", layout({ { kind = "column", children = spacers(600) }, { kind = "column", children = spacers(600) } }))
end

local function frame(g, name, children)
  ui:begin_frame(g)
  T.call(name, function() return ui:column(name, { x = 0, y = 0, w = 200, h = 200, children = children }) end)
  ui:end_frame()
  T.value(name .. ".focus", ui:focused() or "none")
end

function on_draw(ctx, g)
  g:clear(0)
  frame(g, "frame_15_levels_and_a_control", chain(15, BUTTON))
  frame(g, "frame_16_levels_and_a_control", chain(16, BUTTON))
  frame(g, "frame_16_levels_and_a_spacer", chain(16, SPACER))
  frame(g, "frame_17_levels", chain(17))
  -- 125 buttons, then a column (1) whose third child would be the 129th control
  ui:begin_frame(g)
  for i = 1, 125 do
    ui:button("b" .. i, { x = (i - 1) % 8 * 30, y = 400 + (i - 1) // 8 * 20, w = 20, h = 10 })
  end
  T.call("container_at_limit", function()
    return ui:row("full", { x = 0, y = 0, w = 300, h = 50, children = {
      { kind = "spacer", w = 5 },
      { kind = "button", id = "k1", text = "1", w = 100 },
      { kind = "button", id = "k2", text = "2", w = 100 },
      { kind = "button", id = "k3", text = "3", w = 90 },
    } })
  end)
  ui:end_frame()
  local function tap(name, x)
    T.call(name, function() return ui:handle_event({ type = "pointer", phase = "up", gesture = "tap", x = x, y = 25 }) end)
  end
  tap("container_at_limit.k2", 150)
  tap("container_at_limit.k3", 250)
end
