local T = require("conformance")
local function fill(g, n)
  for _ = 1, n do g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end
end

function on_draw(ctx, g)
  fill(g, 1022)
  g:text_box(0, 100, 200, 200, "l1\nl2\nl3")
  T.call("rect_1023", function() return g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end)
  T.call("rect_1024", function() return g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end)
end
