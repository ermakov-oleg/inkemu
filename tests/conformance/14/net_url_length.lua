local T = require("conformance")

function on_load(ctx)
  local base = "https://u.test/"
  T.call("long1025", function()
    return ctx.net:request({ url = base .. string.rep("a", 1025 - #base) }, function() end)
  end)
end
