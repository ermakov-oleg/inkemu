local T = require("conformance")

function on_load(ctx)
  local B = ctx.base64
  T.call("url_alphabet", function() return B:encode("\251\255\191", { url = true }) end)
  T.call("url_no_padding_2", function() return B:encode("ab", { url = true }) end)
  T.call("url_no_padding_1", function() return B:encode("a", { url = true }) end)
end
