local T = require("conformance")

function on_load(ctx)
  local ms = ctx.clock:millis()
  T.type("type", ms)
  T.value("in_range", ms >= 0 and ms <= 2147483647)
  ctx:after(2000, function(ctx)
    local later = ctx.clock:millis()
    T.value("elapsed_at_least_2000", ((later - ms) & 0x7FFFFFFF) >= 2000)
  end)
end
