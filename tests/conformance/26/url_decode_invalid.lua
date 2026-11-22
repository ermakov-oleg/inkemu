local T = require("conformance")

function on_load(ctx)
  local U = ctx.url
  T.call("bad_hex", function() return U:decode("%zz") end)
  T.call("truncated", function() return U:decode("%4") end)
end
