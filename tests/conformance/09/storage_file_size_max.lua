local T = require("conformance")

function on_load(ctx)
  local S = ctx.storage
  local big = string.rep("x", 262144)
  T.call("write_max", function() return S:write("max", big) end)
  T.call("size_max", function() return S:size("max") end)
end
