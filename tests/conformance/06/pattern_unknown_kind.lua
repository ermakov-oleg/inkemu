local T = require("conformance")

function on_load(ctx)
  T.value("capabilities.pattern", ctx.capabilities.pattern)
end

function on_draw(ctx, g)
  g:clear(0)
  g:pattern(10, 10, 40, 40, { kind = "bogus", density = 100 })
  g:pattern(100, 10, 40, 40, { kind = "dot", density = 100 })
end
