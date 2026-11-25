local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

-- the calls ui:switch draws for a box, as the contract's capsule rules give them
local function expected(ctx, x, y, w, h, on, text_box)
  local p = w > 32 and 8 or 0
  local th = math.min(h, 34)
  local tw = math.min(w - 2 * p, 2 * th - 4)
  th = math.min(th, tw)
  local cx, cy = x + w - p - tw, y + (h - th) // 2
  local out = { L.call("rect", cx, cy, tw, th, { fill = on, line_width = 2, radius = th // 2 }) }
  if on then
    out[#out + 1] = L.call("circle", cx + tw - th // 2, cy + th // 2, th // 2 - 5, { color = 0, fill = true })
  else
    out[#out + 1] = L.call("circle", cx + th // 2, cy + th // 2, th // 2 - 6, { fill = true })
  end
  if text_box then
    out[#out + 1] = text_box(x + p, cx - 12 - (x + p))
  end
  return table.concat(out, " ")
end

local function drawn(g, id, o)
  ui:begin_frame(g)
  ui:switch(id, o)
  local log = L.start(g)
  ui:end_frame()
  return L.text(log)
end

function on_draw(ctx, g)
  g:clear(0)
  local font = ctx.fonts.medium
  local function plain(h, on)
    return function(tx, room)
      return L.call("text_box", tx, 100, room, h, "Wi-Fi", { bold = true, font = font, overflow = "ellipsis",
        valign = "middle", wrap = "none" })
    end
  end
  T.value("off", drawn(g, "a", { x = 20, y = 100, w = 440, h = 56, text = "Wi-Fi" })
    == expected(ctx, 20, 100, 440, 56, false, plain(56)))
  T.value("on", drawn(g, "a", { x = 20, y = 100, w = 440, h = 56, text = "Wi-Fi", selected = true })
    == expected(ctx, 20, 100, 440, 56, true, plain(56)))
  -- a switch 40 tall and 40 wide: the capsule shrinks to the width left and no text fits
  T.value("narrow", drawn(g, "a", { x = 20, y = 100, w = 40, h = 40, text = "Wi-Fi" })
    == expected(ctx, 20, 100, 40, 40, false, nil))
  -- 20 tall: the capsule is as tall as the box
  T.value("short", drawn(g, "a", { x = 20, y = 100, w = 80, h = 20, text = "Wi-Fi" })
    == expected(ctx, 20, 100, 80, 20, false, plain(20)))
  -- the note takes the lower half, the text the upper, each two pixels short of the middle line
  T.value("note", drawn(g, "a", { x = 20, y = 100, w = 440, h = 70, text = "Wi-Fi", note = "Flip" })
    == expected(ctx, 20, 100, 440, 70, false, function(tx, room)
      return L.call("text_box", tx, 100, room, 33, "Wi-Fi", { bold = true, font = font, overflow = "ellipsis",
          valign = "bottom", wrap = "none" }) .. " "
        .. L.call("text_box", tx, 137, room, 33, "Flip", { color = 10, font = ctx.fonts.small,
          overflow = "ellipsis", valign = "top", wrap = "none" })
    end))
  T.value("note_empty", drawn(g, "a", { x = 20, y = 100, w = 440, h = 70, text = "Wi-Fi", note = "" })
    == expected(ctx, 20, 100, 440, 70, false, plain(70)))
  ui:begin_frame(g)
  T.call("note_number", function()
    return ui:switch("n", { x = 20, y = 100, w = 440, h = 56, text = "x", note = 5 })
  end)
  ui:end_frame()
  -- the frame left on screen: an on switch, ink capsule and a paper circle
  drawn(g, "a", { x = 20, y = 100, w = 440, h = 56, text = "Wi-Fi", selected = true })
end
