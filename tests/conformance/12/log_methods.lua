local T = require("conformance")

function on_load(ctx)
  T.value("info_table_raises", not pcall(function() ctx.log:info({}) end))
  T.value("warn_nil_raises", not pcall(function() ctx.log:warn(nil) end))
  T.value("error_bool_raises", not pcall(function() ctx.log:error(true) end))
end
