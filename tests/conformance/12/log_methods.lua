local T = require("conformance")

function on_load(ctx)
  T.call("info", function() return ctx.log:info("conformance info") end)
  T.call("warn", function() return ctx.log:warn("conformance warn") end)
  T.call("error", function() return ctx.log:error("conformance error") end)
  T.value("info_table_raises", not pcall(function() ctx.log:info({}) end))
  T.value("warn_nil_raises", not pcall(function() ctx.log:warn(nil) end))
  T.value("error_bool_raises", not pcall(function() ctx.log:error(true) end))
end
