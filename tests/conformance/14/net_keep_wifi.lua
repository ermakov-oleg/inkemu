local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  T.call("zero", function() return N:keep_wifi(0) end)
  T.call("forever", function() return N:keep_wifi(-1) end)
  T.call("default", function() return N:keep_wifi(300000) end)
  T.call("max", function() return N:keep_wifi(2147483647) end)
  T.call("below", function() return N:keep_wifi(-2) end)
  T.call("fraction", function() return N:keep_wifi(1.5) end)
  T.call("above", function() return N:keep_wifi(2147483648) end)
end
