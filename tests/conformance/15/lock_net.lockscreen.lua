local T = require("conformance")

function on_load(ctx)
  T.value("lock_net", ctx.lock.net)
  T.value("has_net", ctx.net ~= nil)
end

function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end
