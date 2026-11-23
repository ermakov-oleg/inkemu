local T = require("conformance")

local function flat(n)
  local t = {}
  for i = 1, n do t[#t + 1] = 5 + i; t[#t + 1] = 5 + i end
  return t
end

function on_draw(ctx, g)
  T.call("two_points", function() return g:polyline(flat(2)) end)
  T.call("max_points", function() return g:polyline(flat(128)) end)
end
