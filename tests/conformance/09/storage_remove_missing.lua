local T = require("conformance")

function on_load(ctx)
  local S = ctx.storage
  T.call("remove_missing", function() return S:remove("nothing") end)
  S:write("here", "x")
  S:remove("here")
  T.call("remove_again", function() return S:remove("here") end)
end
