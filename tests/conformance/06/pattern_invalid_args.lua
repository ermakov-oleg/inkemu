local T = require("conformance")

function on_draw(ctx, g)
  g:clear(0)
  T.call("bogus_kind", function() return g:pattern(10, 10, 40, 40, { kind = "bogus", density = 100 }) end)
  T.call("density_101", function() return g:pattern(100, 10, 40, 40, { kind = "dot", density = 101 }) end)
  T.call("density_negative", function() return g:pattern(190, 10, 40, 40, { kind = "dot", density = -1 }) end)
end
