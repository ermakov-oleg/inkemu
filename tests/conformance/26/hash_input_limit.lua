local T = require("conformance")

function on_load(ctx)
  local H = ctx.hash
  local big = string.rep("a", 1048577)
  T.call("sha256_over", function() return H:sha256(big) end)
  T.call("sha1_over", function() return H:sha1(big) end)
  T.call("md5_over", function() return H:md5(big) end)
  T.call("crc32_over", function() return H:crc32(big) end)
  T.call("hmac_over", function() return H:hmac_sha256("k", big) end)
end
