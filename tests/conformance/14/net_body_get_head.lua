local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  local noop = function() end
  T.call("get_body", function() return N:request({ url = "https://b.test/a", body = "x" }, noop) end)
  T.call("head_body", function() return N:request({ url = "https://b.test/a", method = "HEAD", body = "x" }, noop) end)
end
