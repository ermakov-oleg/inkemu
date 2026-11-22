local T = require("conformance")
local snap, seq = "none", {}

function on_load(ctx)
  if not ctx.lock.net then return end
  ctx.net:request({ url = "https://x.test/lock" }, function(ctx, res) snap = res.body end)
end

function on_draw(ctx, g)
  seq[#seq + 1] = snap
  T.value("draws", table.concat(seq, ","))
  g:rect(10, 10, 20, 20, { fill = true })
end
