local T = require("conformance")

function on_load(ctx)
  T.call("extend", function() return ctx.budget:extend() end)
end

function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end
