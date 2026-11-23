local T = require("conformance")
local function fill(g, n)
  for _ = 1, n do g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end
end

function on_draw(ctx, g)
  g:rect(10, 10, 20, 20, { fill = true })
  fill(g, 1023)
  T.call("cmd_1025", function() return g:rect(100, 100, 20, 20, { fill = true }) end)
  T.call("cmd_1026", function() return g:rect(100, 100, 20, 20, { fill = true }) end)
end
