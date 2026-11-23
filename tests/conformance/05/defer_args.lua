local T = require("conformance")

function on_load(ctx)
  T.call("min", function() return ctx:defer_full_refresh(1) end)
  T.call("max", function() return ctx:defer_full_refresh(60000) end)
  T.call("zero", function() return ctx:defer_full_refresh(0) end)
  T.call("over", function() return ctx:defer_full_refresh(60001) end)
  T.call("negative", function() return ctx:defer_full_refresh(-5) end)
end
