local T = require("conformance")

function on_load(ctx)
  T.value("reason", ctx.lock.reason)
  T.value("elapsed", ctx.lock.elapsed_sec)
end

function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end
