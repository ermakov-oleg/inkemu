local T = require("conformance")

function on_load(ctx)
  T.call("info", function() return ctx.log:info("conformance info") end)
  T.call("warn", function() return ctx.log:warn("conformance warn") end)
  T.call("error", function() return ctx.log:error("conformance error") end)
end
