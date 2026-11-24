local T = require("conformance")

function on_load(ctx)
  T.call("unknown", function() return ctx.system:act("bogus") end)
  T.value("number_raises", not pcall(function() return ctx.system:act(1) end))
  T.value("nil_raises", not pcall(function() return ctx.system:act() end))
end
