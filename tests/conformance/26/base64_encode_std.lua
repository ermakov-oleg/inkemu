local T = require("conformance")

function on_load(ctx)
  local B = ctx.base64
  T.call("std_padding_1", function() return B:encode("a") end)
  T.call("url_empty", function() return B:encode("", { url = true }) end)
  T.call("url_false", function() return B:encode("\251\255\191", { url = false }) end)
end
