local T = require("conformance")

local function flat(n)
  local t = {}
  for i = 1, n do t[#t + 1] = 5 + i; t[#t + 1] = 5 + i end
  return t
end

function on_draw(ctx, g)
  T.call("two_points", function() return g:polygon(flat(2)) end)
  T.call("empty", function() return g:polygon({}) end)
  T.call("odd_array", function() return g:polygon({ 10, 10, 20, 20, 30, 10, 40 }) end)
  T.call("three_points", function() return g:polygon(flat(3)) end)
  T.call("max_points", function() return g:polygon(flat(128)) end)
end
