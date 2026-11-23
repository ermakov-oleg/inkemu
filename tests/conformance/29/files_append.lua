local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  T.call("append_new", function() return F:append("log", "a") end)
  T.call("append_more", function() return F:append("log", "bc") end)
  T.call("read", function() return F:read("log") end)
  T.call("size", function() return F:size("log") end)
  T.call("append_empty", function() return F:append("log", "") end)
end
