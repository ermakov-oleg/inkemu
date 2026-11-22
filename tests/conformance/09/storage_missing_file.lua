local T = require("conformance")

function on_load(ctx)
  local S = ctx.storage
  T.call("read", function() return S:read("nothing") end)
  T.call("size", function() return S:size("nothing") end)
  T.call("exists", function() return S:exists("nothing") end)
end
