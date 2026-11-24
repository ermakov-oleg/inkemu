local T = require("conformance")

function on_load(ctx)
  local H = ctx.hash
  T.call("probe_km", function() return T.hex(H:hmac_sha256("k", "m")) end)
  T.call("rfc4231_2", function() return T.hex(H:hmac_sha256("Jefe", "what do ya want for nothing?")) end)
  T.call("rfc4231_6", function()
    return T.hex(H:hmac_sha256(string.rep("\170", 131), "Test Using Larger Than Block-Size Key - Hash Key First"))
  end)
  T.call("len", function() return #H:hmac_sha256("k", "m") end)
end
