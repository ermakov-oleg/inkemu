local T = require("conformance")

function on_load(ctx)
  T.call("plain", function() return ctx:invalidate() end)
  T.call("full", function() return ctx:invalidate("full") end)
end
