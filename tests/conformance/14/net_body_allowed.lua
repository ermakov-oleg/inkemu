local T = require("conformance")

function on_load(ctx)
  local job = ctx.net:request({ url = "https://b.test/p", method = "PUT", body = "x" }, function() end)
  T.value("put_body_started", job ~= nil)
end
