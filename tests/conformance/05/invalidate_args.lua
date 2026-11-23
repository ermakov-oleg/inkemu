local T = require("conformance")

function on_load(ctx)
  T.call("bogus", function() return ctx:invalidate("bogus") end)
  T.call("number", function() return ctx:invalidate(1) end)
end
