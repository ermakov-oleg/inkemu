local T = require("conformance")

function on_load(ctx)
  local R = ctx.random
  T.call("zero", function() return R:bytes(0) end)
  T.call("over", function() return R:bytes(1025) end)
end
