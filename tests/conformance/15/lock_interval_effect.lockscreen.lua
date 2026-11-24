local T = require("conformance")

function on_load(ctx)
  local n = tonumber(ctx.storage:read("n") or "0") + 1
  ctx.storage:write("n", tostring(n))
  if n == 1 then ctx.lock:set_interval(120) end
  T.value("sessions", n)
  T.value("interval", ctx.lock.interval_sec)
end

function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end
