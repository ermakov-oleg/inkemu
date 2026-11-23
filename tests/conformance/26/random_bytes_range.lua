local T = require("conformance")

function on_load(ctx)
  local R = ctx.random
  T.call("one", function() return #R:bytes(1) end)
  T.call("max", function() return #R:bytes(1024) end)
  T.call("negative", function() return R:bytes(-1) end)
  T.value("differ", R:bytes(16) ~= R:bytes(16))
end
