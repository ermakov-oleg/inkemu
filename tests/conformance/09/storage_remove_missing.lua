local T = require("conformance")

function on_load(ctx)
  local S = ctx.storage
  T.call("remove_missing", function() return S:remove("nothing") end)
  S:write("here", "x")
  T.call("remove_existing", function() return S:remove("here") end)
  T.call("exists_after", function() return S:exists("here") end)
  T.call("remove_again", function() return S:remove("here") end)
end
