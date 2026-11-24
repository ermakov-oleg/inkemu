local T = require("conformance")

function on_load(ctx)
  T.call("empty", function() return ctx.net:request({ url = "https://c.test/a", method = "" }, function() end) end)
end
