local T = require("conformance")
local snap = "none"

function on_load(ctx)
  ctx.net:request({ url = "https://x.test/lock" }, function(ctx, res) snap = res.body end)
  ctx:after(2000, function(ctx) ctx.lock:done() end)
end

function on_draw(ctx, g)
  if ctx.lock.reason == "timer" then T.value("timer_draw", snap) end
  g:rect(10, 10, 20, 20, { fill = true })
end
