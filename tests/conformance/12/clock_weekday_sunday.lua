local T = require("conformance")

function on_load(ctx)
  T.call("weekday", function() return ctx.clock:now().weekday end)
end
