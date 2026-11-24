local T = require("conformance")

function on_load(ctx)
  local H = ctx.hash
  local max = string.rep("a", 1048576)
  T.call("sha256", function() return H:sha256(max) ~= nil end)
  T.call("sha1", function() return H:sha1(max) ~= nil end)
  T.call("md5", function() return H:md5(max) ~= nil end)
  T.call("crc32", function() return H:crc32(max) ~= nil end)
  T.call("hmac", function() return H:hmac_sha256("k", max) ~= nil end)
end
