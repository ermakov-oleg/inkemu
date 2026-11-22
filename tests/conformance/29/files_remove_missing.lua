local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  T.call("remove_missing", function() return F:remove("nothing") end)
  F:write("here", "x")
  F:remove("here")
  T.call("remove_again", function() return F:remove("here") end)
end
