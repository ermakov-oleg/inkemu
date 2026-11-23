local T = require("conformance")

local function nret(...) return select("#", ...) end

function on_draw(ctx, g)
  local s = string.rep("a", 8192)
  T.call("eight_texts", function()
    for i = 1, 8 do
      if nret(g:text(0, 0, s)) > 0 then return i end
    end
    return 0
  end)
end
