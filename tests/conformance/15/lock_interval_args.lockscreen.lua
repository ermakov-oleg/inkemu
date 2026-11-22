local T = require("conformance")

function on_load(ctx)
  T.call("set_29", function() return ctx.lock:set_interval(29) end)
  T.call("set_3601", function() return ctx.lock:set_interval(3601) end)
  T.call("set_fraction", function() return ctx.lock:set_interval(45.5) end)
  T.call("set_30", function() return ctx.lock:set_interval(30) end)
  T.call("set_3600", function() return ctx.lock:set_interval(3600) end)
end

function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end
