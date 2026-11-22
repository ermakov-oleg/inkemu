local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  T.value("string_raises", not pcall(function() N:keep_wifi("5") end))
  T.value("missing_raises", not pcall(function() N:keep_wifi() end))
end
