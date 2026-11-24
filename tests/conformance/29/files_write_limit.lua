local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  local big = string.rep("y", 262144)
  T.call("write_over", function() return F:write("over", big .. "z") end)
  T.call("size_over", function() return F:size("over") end)
  F:write("small", "old")
  T.call("overwrite_over", function() return F:write("small", big .. "z") end)
  T.call("small_unchanged", function() return F:read("small") == "old" end)
end
