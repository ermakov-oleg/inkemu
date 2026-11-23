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
  -- all children are counted before any field is checked
  local bad_first = spacers(1025)
  bad_first[1] = { kind = "spacer", h = 0.5 }
  T.value("layout.1025_children_bad_first", layout(bad_first))
  -- children are counted over the whole tree
  T.value("layout.2_x_600_children", layout({ { kind = "column", children = spacers(600) }, { kind = "column", children = spacers(600) } }))
end

local function frame(g, name, children)
  ui:begin_frame(g)
  T.call(name, function() return ui:column(name, { x = 0, y = 0, w = 200, h = 200, children = children }) end)
  ui:end_frame()
  T.value(name .. ".focus", ui:focused() or "none")
end

local function tap(name, x, y)
  T.call(name, function() return ui:handle_event({ type = "pointer", phase = "up", gesture = "tap", x = x, y = y }) end)
end

function on_draw(ctx, g)
  g:clear(0)
  -- children are checked and registered one by one: a bad field, or a child placed past 32767, ends the call
  -- there and keeps the children before it
  ui:begin_frame(g)
  T.call("bad_child", function()
    return ui:row("bad", { x = 0, y = 700, w = 300, h = 40, children = {
      { kind = "button", id = "p1", text = "1", w = 100 },
      { kind = "button", id = "p2", text = "2", w = 100, enabled = "no" },
      { kind = "button", id = "p3", text = "3", w = 100 },
    } })
  end)
  ui:end_frame()
  tap("bad_child.before", 50, 720)
  tap("bad_child.after", 250, 720)
  ui:begin_frame(g)
  T.call("child_past_32767", function()
    return ui:row("far", { x = 0, y = 700, w = 300, h = 40, children = {
      { kind = "button", id = "q1", text = "1", w = 20000, shrink = 0 },
      { kind = "button", id = "q2", text = "2", w = 20000, shrink = 0 },
      { kind = "button", id = "q3", text = "3", w = 20000, shrink = 0 },
    } })
  end)
  ui:end_frame()
  tap("child_past_32767.before", 50, 720)
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
  tap("container_at_limit.k2", 150, 25)
  tap("container_at_limit.k3", 250, 25)
end
