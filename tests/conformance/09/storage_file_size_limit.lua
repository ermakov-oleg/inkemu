local T = require("conformance")

function on_load(ctx)
  local S = ctx.storage
  local big = string.rep("x", 262144)
  T.call("write_over", function() return S:write("over", big .. "y") end)
  T.call("size_over", function() return S:size("over") end)
  S:write("small", "old")
  T.call("overwrite_over", function() return S:write("small", big .. "y") end)
  T.call("small_unchanged", function() return S:read("small") == "old" end)
end
