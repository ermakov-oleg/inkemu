local T = require("conformance")

function on_load(ctx)
  local H = ctx.hash
  T.call("sha1_abc_len", function() return #H:sha1("abc") end)
  T.call("sha1_abc_hex", function() return T.hex(H:sha1("abc")) end)
  T.call("sha1_empty_hex", function() return T.hex(H:sha1("")) end)
end
