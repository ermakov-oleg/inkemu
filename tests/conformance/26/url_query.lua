local T = require("conformance")

function on_load(ctx)
  local U = ctx.url
  T.call("sorted", function() return U:query({ q = "paper", page = 2 }) end)
  T.call("bool_number", function() return U:query({ a = true, b = false, c = 7 }) end)
  T.call("encoded_value", function() return U:query({ k = "a b" }) end)
  T.call("byte_order", function() return U:query({ b = 1, B = 2, a = 3 }) end)
  T.call("empty", function() return U:query({}) end)
end
