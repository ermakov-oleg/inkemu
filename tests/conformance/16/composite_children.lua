local T = require("conformance")
local L = require("canvas_log")
local B = require("layout_box")
local ui
local rows

function on_load(ctx)
  ui = ctx.ui:create()
end

-- what the list's row callback saw, as "index@x,y,w,h"
local function draw(_, r, i)
  rows[#rows + 1] = string.format("%d@%d,%d,%d,%d", i, r.x, r.y, r.w, r.h)
end

local function stepper(extra)
  local o = { kind = "stepper", id = "st", min = 1, max = 5, value = 2, h = 56 }
  for k, v in pairs(extra or {}) do o[k] = v end
  return o
end

local function pager(extra)
  local o = { kind = "pager", id = "pg", page = 2, pages = 3, h = 40 }
  for k, v in pairs(extra or {}) do o[k] = v end
  return o
end

local function list(extra)
  local o = { kind = "list", id = "ls", count = 9, row_h = 40, page = 1, h = 120, draw = draw }
  for k, v in pairs(extra or {}) do o[k] = v end
  return o
end

-- one column of `children` in a frame: the call's return and the canvas calls of end_frame
local function column(g, name, children)
  rows = {}
  ui:begin_frame(g)
  T.call(name, function()
    return ui:column("c", { x = 0, y = 0, w = 480, h = 400, gap = 10, children = children })
  end)
  local log = L.start(g)
  ui:end_frame()
  return L.text(log)
end

local function tap(x, y)
  return { type = "pointer", phase = "up", x = x, y = y, down_x = x, down_y = y, gesture = "tap", time_ms = 0 }
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then return end
  done = true
  local med = ctx.fonts.medium
  local function box(x, y, w, h)
    return L.call("rect", x, y, w, h, { fill = false, radius = 0 })
  end
  -- the minus and plus bars of a part of side s at x, y: max(6, s * 9 // 20) long and 3 thick, centred
  local function signs(x, y, s, plus)
    local len = math.max(6, s * 9 // 20)
    local cx, cy = x + s // 2, y + s // 2
    local out = L.call("rect", cx - len // 2, cy - 1, len, 3, { color = 15, fill = true })
    if plus then
      out = out .. " " .. L.call("rect", cx - 1, cy - len // 2, 3, len, { color = 15, fill = true })
    end
    return out
  end
  local function text(x, y, w, h, s, tabular)
    return L.call("text_box", x, y, w, h, s, { align = "center", color = 15, font = med, overflow = "ellipsis",
      tabular = tabular, valign = "middle", wrap = "none" })
  end
  -- stretched across the 480 px column: the stepper's parts at both ends, the pager's 40 px buttons, three rows
  local drawn = column(g, "stretched", { stepper(), pager(), list() })
  T.value("stretched_drawn", drawn == table.concat({
    box(0, 0, 56, 56), signs(0, 0, 56, false), box(424, 0, 56, 56), signs(424, 0, 56, true),
    text(56, 0, 368, 56, "2", true),
    box(0, 66, 40, 40), text(0, 66, 40, 40, "<"), box(440, 66, 40, 40), text(440, 66, 40, 40, ">"),
    text(40, 66, 400, 40, "2/3", true),
  }, " "))
  T.value("stretched_rows", table.concat(rows, " "))
  T.call("tap_plus", function() return ui:handle_event(tap(470, 30)) end)
  -- sized children keep their own width
  drawn = column(g, "sized", { stepper({ w = 112 }), pager({ w = 200 }), list({ w = 200 }) })
  T.value("sized_drawn", drawn == table.concat({
    box(0, 0, 56, 56), signs(0, 0, 56, false), box(56, 0, 56, 56), signs(56, 0, 56, true),
    box(0, 66, 40, 40), text(0, 66, 40, 40, "<"), box(160, 66, 40, 40), text(160, 66, 40, 40, ">"),
    text(40, 66, 120, 40, "2/3", true),
  }, " "))
  T.value("sized_rows", table.concat(rows, " "))
  -- in a grid of two 240 px columns and 100 px rows
  local cells = { stepper(), pager(), list() }
  for _, o in ipairs(cells) do o.h = nil end
  rows = {}
  ui:begin_frame(g)
  T.call("grid", function()
    return ui:grid("g", { x = 0, y = 0, w = 480, h = 300, columns = 2, auto_rows = 100, children = cells })
  end)
  local log = L.start(g)
  ui:end_frame()
  T.value("grid_parts", string.find(L.text(log), box(140, 0, 100, 100), 1, true) ~= nil)
  T.value("grid_rows", table.concat(rows, " "))
  -- a laid-out box that breaks a kind's box rule refuses the call, and nothing is drawn
  T.value("stepper_narrow_drawn", column(g, "stepper_narrow", { stepper({ w = 80 }) }))
  T.value("pager_button_w_drawn", column(g, "pager_button_w", { pager({ w = 60, button_w = 40 }) }))
  T.value("list_row_h_drawn", column(g, "list_row_h", { list({ h = 30 }) }))
  column(g, "value_out_of_range", { stepper({ value = 9 }) })
  -- a hidden child's box is not held to those rules
  T.value("hidden_drawn", column(g, "hidden", {
    stepper({ w = 80, hidden = true }), list({ h = 30, hidden = true }),
    { kind = "button", id = "b", text = "b", h = 40 },
  }) == box(0, 0, 480, 40) .. " " .. text(0, 0, 480, 40, "b"))
  -- ui:layout places such children without the box rules; a stepper cannot fit its width
  T.value("layout_small", B.text(ui:layout({ x = 0, y = 0, w = 480, h = 400 }, { kind = "column", children = {
    stepper({ w = 80 }), list({ h = 30 }) } })))
  T.call("layout_stepper_fit", function()
    return ui:layout({ x = 0, y = 0, w = 480, h = "fit" }, { kind = "row", children = { stepper({ w = "fit" }) } })
  end)
end
