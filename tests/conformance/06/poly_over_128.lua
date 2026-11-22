local T = require("conformance")

local function flat(n)
  local t = {}
  for i = 1, n do t[#t + 1] = 5 + i; t[#t + 1] = 5 + i end
  return t
end

function on_draw(ctx, g)
  T.call("polyline_129", function() return g:polyline(flat(129)) end)
  T.call("polygon_129", function() return g:polygon(flat(129)) end)
end
