local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

local function filled(x, y, w, h, color, radius)
  return L.call("rect", x, y, w, h, { color = color, fill = true, radius = radius })
end

local function dots(x, y, w, h, density)
  return L.call("pattern", x, y, w, h, { color = 15, density = density, kind = "dot" })
end

-- a halftone block as §6 rounds a rect: paper with the radius, then one dot row per corner row (top, then its
-- mirror at the bottom), then the middle
local function rounded(x, y, w, h, r, density)
  local out = { filled(x, y, w, h, 0, r) }
  for dy = 0, r - 1 do
    local inset = r - math.floor(math.sqrt(r * r - (r - dy - 0.5) ^ 2) + 0.5)
    out[#out + 1] = dots(x + inset, y + dy, w - 2 * inset, 1, density)
    out[#out + 1] = dots(x + inset, y + h - 1 - dy, w - 2 * inset, 1, density)
  end
  out[#out + 1] = dots(x, y + r, w, h - 2 * r, density)
  return table.concat(out, " ")
end

local function box(id, x, y, more)
  local o = { x = x, y = y, w = 100, h = 60, children = { { kind = "spacer" } } }
  for k, v in pairs(more) do o[k] = v end
  ui:column(id, o)
end

local function frame(g, body)
  ui:begin_frame(g)
  body()
  local log = L.start(g)
  ui:end_frame()
  return L.text(log)
end

function on_draw(ctx, g)
  g:clear(0)
  -- the four colour bands, without a radius
  local got = frame(g, function()
    box("c1", 0, 0, { background = 1 })
    box("c5", 110, 0, { background = 5 })
    box("c10", 220, 0, { background = 10 })
    box("c14", 330, 0, { background = 14 })
  end)
  T.value("bands", got == table.concat({
    filled(0, 0, 100, 60, 0, 0),
    filled(110, 0, 100, 60, 0, 0), dots(110, 0, 100, 60, 38),
    filled(220, 0, 100, 60, 0, 0), dots(220, 0, 100, 60, 69),
    filled(330, 0, 100, 60, 15, 0),
  }, " "))
  -- a rounded halftone fill takes 2r + 2 calls; the border comes after the fill
  got = frame(g, function()
    box("r6", 0, 70, { background = 5, radius = 6 })
    box("r8", 110, 70, { background = 10, radius = 8, border = true })
    box("b3", 220, 70, { border = true, line_width = 3, border_color = 8, radius = 10 })
    box("b0", 330, 70, { border = false, line_width = 4 })
  end)
  T.value("rounded_and_border", got == table.concat({
    rounded(0, 70, 100, 60, 6, 38),
    rounded(110, 70, 100, 60, 8, 69), L.call("rect", 110, 70, 100, 60, { color = 15, line_width = 1, radius = 8 }),
    L.call("rect", 220, 70, 100, 60, { color = 8, line_width = 3, radius = 10 }),
  }, " "))
  -- dividers halfway in the gap (floored), across the container's box less the inset, none next to a spacer;
  -- they are drawn before the children
  got = frame(g, function()
    ui:column("dc", { x = 0, y = 140, w = 200, h = 300, gap = 10, divider = true, children = {
      { kind = "region", id = "a", h = 40 }, { kind = "region", id = "b", h = 40 },
      { kind = "spacer", h = 4 }, { kind = "region", id = "c", h = 40 } } })
    ui:row("dr", { x = 220, y = 140, w = 250, h = 60, gap = 9, divider = { line_width = 3, color = 6, inset = 10 },
      children = { { kind = "region", id = "d", w = 60 }, { kind = "region", id = "e", w = 60 } } })
    ui:column("d0", { x = 220, y = 220, w = 200, h = 200, divider = { line_width = 2, color = 14 }, children = {
      { kind = "region", id = "f", h = 56 }, { kind = "region", id = "h", h = 56 } } })
  end)
  T.value("dividers", got == table.concat({
    filled(0, 140 + 40 + (10 - 1) // 2, 200, 1, 15),
    filled(220 + 60 + (9 - 3) // 2, 150, 3, 40, 0), dots(220 + 60 + (9 - 3) // 2, 150, 3, 40, 38),
    -- gap 0: the 2 px line starts 1 px inside the first child
    filled(220, 220 + 56 + (0 - 2) // 2, 200, 2, 15),
  }, " "))
end
