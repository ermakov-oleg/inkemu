local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  F:write("d", "0123456789")
  T.call("whole", function() return F:read("d") end)
  T.call("missing", function() return F:read("nothing", 0, 4) end)
end
