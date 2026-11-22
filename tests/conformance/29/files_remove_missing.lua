local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  T.call("remove_missing", function() return F:remove("nothing") end)
  F:write("here", "x")
  T.call("remove_existing", function() return F:remove("here") end)
  T.call("exists_after", function() return F:exists("here") end)
  T.call("remove_again", function() return F:remove("here") end)
end
