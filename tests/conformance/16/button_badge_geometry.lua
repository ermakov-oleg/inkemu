local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

-- the badge's three calls for a control at x, y, w with badge text `text`
local function badge(x, y, w, text)
  local small = ctx.fonts.small
  local bh = small.line_height + 4
  local bw = math.max(bh, small:measure(text) + 8)
  local bx, by = x + w - 4 - bw, y + 4
  return L.call("rect", bx - 2, by - 2, bw + 4, bh + 4, { color = 0, fill = true, radius = bh // 2 + 2 }) .. " "
    .. L.call("rect", bx, by, bw, bh, { color = 15, fill = true, radius = bh // 2 }) .. " "
    .. L.call("text_box", bx, by, bw, bh, text, { align = "center", color = 0, font = small, overflow = "ellipsis",
      valign = "middle", wrap = "none" })
end

function on_draw(ctx, g)
  g:clear(0)
  local bh = ctx.fonts.small.line_height + 4
  -- placed so that the badge's middle row is y = 200 and its right edge x = 400, whatever the font
  local y = 200 - 4 - bh // 2
  ui:begin_frame(g)
  ui:button("b", { x = 200, y = y, w = 205, h = 80, text = "", badge = "12345" })
  ui:button("none", { x = 20, y = 400, w = 100, h = 40, text = "", badge = "" })
  ui:region("r", { x = 20, y = 500, w = 100, h = 60, badge = "7", draw = function() L.mark("draw") end })
  local log = L.start(g)
  ui:end_frame()
  local text = L.call("text_box", 200, y, 205, 80, "", { align = "center", color = 15, font = ctx.fonts.medium,
    overflow = "ellipsis", valign = "middle", wrap = "none" })
  T.value("badge.after_text", table.concat(log, " ", 2, 5) == text .. " " .. badge(200, y, 205, "12345"))
  T.value("empty_badge.none", log[6] == L.call("rect", 20, 400, 100, 40, { fill = false, radius = 0 })
    and log[7] == L.call("text_box", 20, 400, 100, 40, "", { align = "center", color = 15, font = ctx.fonts.medium,
      overflow = "ellipsis", valign = "middle", wrap = "none" }) and log[8] == "draw")
  T.value("region.after_draw", table.concat(log, " ", 8, #log) == "draw " .. badge(20, 500, 100, "7"))
  T.value("calls", L.text(log))
end
