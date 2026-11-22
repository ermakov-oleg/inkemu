local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  local big = string.rep("y", 262144)
  T.call("write_max", function() return F:write("max", big) end)
  T.call("size_max", function() return F:size("max") end)
end
