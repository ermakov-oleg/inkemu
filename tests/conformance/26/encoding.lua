local T = require("conformance")

function on_load(ctx)
  T.call("sha256_empty.len", function() return #ctx.hash:sha256("") end)
  T.call("sha256_empty.hex", function() return T.hex(ctx.hash:sha256("")) end)
  T.call("md5_empty.len", function() return #ctx.hash:md5("") end)
  T.call("md5_abc.hex", function() return T.hex(ctx.hash:md5("abc")) end)
  T.call("hex.encode", function() return ctx.hex:encode("\xab\xcd\xef") end)
  T.call("base64.std", function() return ctx.base64:encode("\xfb\xff\xbf") end)
  T.call("base64.std_padded", function() return ctx.base64:encode("ab") end)
end
