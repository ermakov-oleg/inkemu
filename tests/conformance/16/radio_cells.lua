local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

local ITEMS = { "Alpha", "Beta", "Gamma", "Delta", "Epsilon" }

-- the calls of one radio cell at x, y, w, h showing `item`, with the dot when `on`
local function cell(x, y, w, h, item, on, opts)
  local s = math.min(h, 32)
  local cx, cy = x + s // 2, y + h // 2
  local out = { L.call("circle", cx, cy, s // 2, { line_width = 2 }) }
  if on then
    out[#out + 1] = L.call("circle", cx, cy, (s - 10) // 2, { fill = true })
  end
  local o = { align = "left", color = 15, font = ctx.fonts.medium, overflow = "ellipsis", valign = "middle",
    wrap = "none" }
  for k, v in pairs(opts or {}) do o[k] = v end
  out[#out + 1] = L.call("text_box", x + s + 8, y, w - s - 8, h, item, o)
  return table.concat(out, " ")
end

-- the calls ui:radio draws for `o` (x, y, w, h and items are added) in a frame of its own
local function drawn(g, o)
  local t = { x = 20, y = 100, w = 300, h = 200, items = ITEMS }
  for k, v in pairs(o) do t[k] = v end
  ui:begin_frame(g)
  ui:radio("r", t)
  local log = L.start(g)
  ui:end_frame()
  return L.text(log)
end

-- the cells of `items` in boxes { x, y, w, h }, the `value` one with a dot
local function cells(boxes, value, items, opts)
  local out = {}
  for i, b in ipairs(boxes) do
    out[i] = cell(b[1], b[2], b[3], b[4], (items or ITEMS)[i], i == value, opts)
  end
  return table.concat(out, " ")
end

function on_draw(ctx, g)
  g:clear(0)
  -- one column, gap 0: 200 // 5 = 40 each
  T.value("gap_0", drawn(g, { value = 2 }) == cells({
    { 20, 100, 300, 40 }, { 20, 140, 300, 40 }, { 20, 180, 300, 40 }, { 20, 220, 300, 40 }, { 20, 260, 300, 40 },
  }, 2))
  -- gap 8: starts j * 8 + j * 168 // 5, so sizes 33 and 34 alternate as the rounding falls
  T.value("gap_8", drawn(g, { value = 2, gap = 8 }) == cells({
    { 20, 100, 300, 33 }, { 20, 141, 300, 34 }, { 20, 183, 300, 33 }, { 20, 224, 300, 34 }, { 20, 266, 300, 34 },
  }, 2))
  -- two columns and three rows, gap 6 both ways, filled row by row
  T.value("columns_2", drawn(g, { value = 2, columns = 2, gap = 6, h = 150 }) == cells({
    { 20, 100, 147, 46 }, { 173, 100, 147, 46 }, { 20, 152, 147, 46 }, { 173, 152, 147, 46 }, { 20, 204, 147, 46 },
  }, 2))
  -- cells 150 tall: the circle stays 32 across, centred on the cell; no value, no dot
  T.value("tall_no_value", drawn(g, { h = 300, items = { "One", "Two" } }) == cells({
    { 20, 100, 300, 150 }, { 20, 250, 300, 150 },
  }, nil, { "One", "Two" }))
  T.value("align_font_bold", drawn(g, { value = 2, align = "right", font = ctx.fonts.small, bold = true })
    == cells({
      { 20, 100, 300, 40 }, { 20, 140, 300, 40 }, { 20, 180, 300, 40 }, { 20, 220, 300, 40 }, { 20, 260, 300, 40 },
    }, 2, nil, { align = "right", font = ctx.fonts.small, bold = true }))
  -- the whole radio disabled: drawn as usual, then one pass of dots over its box
  T.value("disabled", drawn(g, { value = 2, enabled = false }) == cells({
    { 20, 100, 300, 40 }, { 20, 140, 300, 40 }, { 20, 180, 300, 40 }, { 20, 220, 300, 40 }, { 20, 260, 300, 40 },
  }, 2) .. " " .. L.call("pattern", 20, 100, 300, 200, { color = 0, density = 50, kind = "dot" }))
  -- the frame left on screen: gap 0, value 2
  drawn(g, { value = 2 })
end
