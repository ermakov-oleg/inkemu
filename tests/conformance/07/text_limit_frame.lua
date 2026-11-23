local T = require("conformance")

function on_draw(ctx, g)
  local s = string.rep("a", 8192)
  for _ = 1, 8 do g:text(0, 0, s) end
  T.call("ninth_byte", function() return g:text(0, 0, "x") end)
end
