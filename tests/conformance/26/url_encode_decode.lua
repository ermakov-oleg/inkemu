local T = require("conformance")

function on_load(ctx)
  local U = ctx.url
  T.call("encode_reserved", function() return U:encode(" !#$%&(") end)
  T.call("encode_unreserved", function() return U:encode("AZaz09-._~") end)
  T.call("encode_utf8", function() return U:encode("\u{43}\u{445}") end)
  T.call("decode", function() return U:decode("a%20b+c%41") end)
end
