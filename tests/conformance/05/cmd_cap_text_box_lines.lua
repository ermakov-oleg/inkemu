local T = require("conformance")
local function fill(g, n)
  for _ = 1, n do g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end
end

local function nret(...) return select("#", ...) end

function on_draw(ctx, g)
  fill(g, 1022)
  -- three lines would make command 1025: refused as a whole, so the counter stays at 1022
  T.call("box_three_lines", function() return g:text_box(0, 100, 200, 200, "l1\nl2\nl3") end)
  T.call("rect_1023", function() return g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end)
  T.call("rect_1024", function() return g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end)
  T.call("rect_1025", function() return g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end)
end
