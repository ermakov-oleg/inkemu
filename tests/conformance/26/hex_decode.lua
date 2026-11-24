local T = require("conformance")

function on_load(ctx)
  local X = ctx.hex
  T.call("odd", function() return X:decode("abc") end)
  T.call("not_hex", function() return X:decode("zz") end)
  T.call("half_bad", function() return X:decode("0g") end)
  T.call("space_even", function() return X:decode("a bc") end)
  T.call("upper", function() return T.hex(X:decode("ABCDEF")) end)
  T.call("mixed", function() return T.hex(X:decode("aBcD")) end)
  T.call("empty", function() return X:decode("") end)
end
