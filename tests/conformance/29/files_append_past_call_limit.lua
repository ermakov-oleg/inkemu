local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  local chunk = string.rep("z", 262144)
  T.call("write", function() return F:write("grow", chunk) end)
  T.call("append", function() return F:append("grow", chunk) end)
  T.call("size", function() return F:size("grow") end)
end
