local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  T.call("write_new", function() return F:write("a", "1") end)
  T.call("write_replace", function() return F:write("a", "22") end)
  T.call("read", function() return F:read("a") end)
end
