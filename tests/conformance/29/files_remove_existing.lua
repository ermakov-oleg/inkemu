local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  F:write("here", "x")
  T.call("remove_existing", function() return F:remove("here") end)
  T.call("exists_after", function() return F:exists("here") end)
end
