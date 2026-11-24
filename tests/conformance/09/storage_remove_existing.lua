local T = require("conformance")

function on_load(ctx)
  local S = ctx.storage
  S:write("here", "x")
  T.call("remove_existing", function() return S:remove("here") end)
  T.call("exists_after", function() return S:exists("here") end)
end
