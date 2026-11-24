local T = require("conformance")

function on_load(ctx)
  local H = ctx.hash
  T.call("empty", function() return H:crc32("") end)
  T.call("a", function() return H:crc32("a") end)
  T.call("abc", function() return H:crc32("abc") end)
  T.call("check", function() return H:crc32("123456789") end)
  T.type("a_type", H:crc32("a"))
  T.type("abc_type", H:crc32("abc"))
end
