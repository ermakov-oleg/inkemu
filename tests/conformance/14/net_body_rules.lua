local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  local noop = function() end
  T.call("post_over", function() return N:request({ url = "https://b.test/a", method = "POST", body = string.rep("x", 65537) }, noop) end)
  T.call("put_over", function() return N:request({ url = "https://b.test/a", method = "PUT", body = string.rep("x", 65537) }, noop) end)
end
