local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

local ITEMS = { "All", "Goods", "Medicine", "Misc" }
-- four cells over 460 px with gap 4: 112 wide, 4 apart
local XS = { 10, 126, 242, 358 }

local function text(ctx, x, w, item, o)
  local t = { align = "center", color = 15, font = ctx.fonts.medium, overflow = "ellipsis", valign = "middle",
    wrap = "none" }
  for k, v in pairs(o or {}) do t[k] = v end
  return L.call("text_box", x, 100, w, 52, item, t)
end

-- the calls of the tabs at y = 100 when `o` is set over the default, in a frame of its own
local function drawn(g, o)
  local t = { x = 10, y = 100, w = 460, h = 52, gap = 4, items = ITEMS, value = 2 }
  for k, v in pairs(o or {}) do t[k] = v end
  ui:begin_frame(g)
  ui:tabs("t", t)
  local log = L.start(g)
  ui:end_frame()
  return L.text(log)
end

local function join(parts)
  return table.concat(parts, " ")
end

local function scene(g, value)
  ui:begin_frame(g)
  ui:button("top", { x = 20, y = 20, w = 200, h = 56, text = "top" })
  ui:tabs("t", { x = 10, y = 100, w = 460, h = 52, gap = 4, items = ITEMS, value = value })
  ui:button("bottom", { x = 20, y = 200, w = 200, h = 56, text = "bottom" })
  ui:end_frame()
end

local function send(name, ev)
  T.call(name, function() return ui:handle_event(ev) end)
end

local function key(name, phase)
  return { type = "key", key = name, phase = phase or "down", time_ms = 0 }
end

local function tap(x)
  return { type = "pointer", phase = "up", x = x, y = 120, down_x = x, down_y = 120, gesture = "tap", time_ms = 0 }
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  local function box(i, fill)
    return L.call("rect", XS[i], 100, 112, 52, { fill = fill, radius = 0 })
  end
  -- the value cell is filled, drawn in bold paper-coloured text; the others keep an empty box
  T.value("fill", drawn(g) == join({
    box(1, false), text(ctx, XS[1], 112, "All"),
    box(2, true), text(ctx, XS[2], 112, "Goods", { bold = true, color = 0 }),
    box(3, false), text(ctx, XS[3], 112, "Medicine"),
    box(4, false), text(ctx, XS[4], 112, "Misc"),
  }))
  T.value("outline", drawn(g, { selected_style = "outline" }) == join({
    box(1, false), text(ctx, XS[1], 112, "All"),
    box(2, false), L.call("rect", XS[2], 100, 112, 52, { line_width = 3, radius = 0 }),
    text(ctx, XS[2], 112, "Goods", { bold = true }),
    box(3, false), text(ctx, XS[3], 112, "Medicine"),
    box(4, false), text(ctx, XS[4], 112, "Misc"),
  }))
  -- a bar 4 px tall on the cell's bottom edge, min(8, w // 6) in from each side
  T.value("underline", drawn(g, { selected_style = "underline" }) == join({
    box(1, false), text(ctx, XS[1], 112, "All"),
    box(2, false), L.call("rect", XS[2] + 8, 148, 96, 4, { fill = true }),
    text(ctx, XS[2], 112, "Goods", { bold = true }),
    box(3, false), text(ctx, XS[3], 112, "Medicine"),
    box(4, false), text(ctx, XS[4], 112, "Misc"),
  }))
  -- borderless: only the filled cell has a box, with the radius
  T.value("borderless", drawn(g, { border = false, radius = 8 }) == join({
    text(ctx, XS[1], 112, "All"),
    L.call("rect", XS[2], 100, 112, 52, { fill = true, radius = 8 }),
    text(ctx, XS[2], 112, "Goods", { bold = true, color = 0 }),
    text(ctx, XS[3], 112, "Medicine"),
    text(ctx, XS[4], 112, "Misc"),
  }))
  T.value("bold_all", drawn(g, { bold = true }) == join({
    box(1, false), text(ctx, XS[1], 112, "All", { bold = true }),
    box(2, true), text(ctx, XS[2], 112, "Goods", { bold = true, color = 0 }),
    box(3, false), text(ctx, XS[3], 112, "Medicine", { bold = true }),
    box(4, false), text(ctx, XS[4], 112, "Misc", { bold = true }),
  }))
  T.value("bold_none", drawn(g, { bold = false }) == join({
    box(1, false), text(ctx, XS[1], 112, "All"),
    box(2, true), text(ctx, XS[2], 112, "Goods", { color = 0 }),
    box(3, false), text(ctx, XS[3], 112, "Medicine"),
    box(4, false), text(ctx, XS[4], 112, "Misc"),
  }))
  T.value("disabled", drawn(g, { enabled = false }) == join({
    box(1, false), text(ctx, XS[1], 112, "All"),
    box(2, true), text(ctx, XS[2], 112, "Goods", { bold = true, color = 0 }),
    box(3, false), text(ctx, XS[3], 112, "Medicine"),
    box(4, false), text(ctx, XS[4], 112, "Misc"),
    L.call("pattern", 10, 100, 460, 52, { color = 0, density = 50, kind = "dot" }),
  }))
  T.call("selected_style_bold", function() return ui:tabs("t", { x = 10, y = 100, w = 460, h = 52, items = ITEMS,
    selected_style = "bold" }) end)
  T.call("value_out_of_range", function() return ui:tabs("t", { x = 10, y = 100, w = 460, h = 52, items = ITEMS,
    value = 5 }) end)
  scene(g, 2)
  if done then return end
  done = true
  send("tap_other_cell", tap(400))
  send("tap_value_cell", tap(150))
  -- the gap between cells 1 and 2 is no part of the tabs
  send("tap_gap", tap(122))
  scene(g, 4)
  send("keys_right", key("right"))
  send("ok_down", key("ok"))
  send("ok_up", key("ok", "up"))
  scene(g, 2)
end
