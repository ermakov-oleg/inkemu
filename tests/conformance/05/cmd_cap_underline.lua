local T = require("conformance")
local function fill(g, n)
  for _ = 1, n do g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end
end

function on_draw(ctx, g)
  fill(g, 1022)
  g:text(10, 10, "hi", { underline = true })
  T.call("rect_after", function() return g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end)
end
