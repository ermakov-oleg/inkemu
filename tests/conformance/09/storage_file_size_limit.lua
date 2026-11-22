local T = require("conformance")

function on_load(ctx)
  local S = ctx.storage
  local big = string.rep("x", 262144)
  T.call("write_max", function() return S:write("max", big) end)
  T.call("size_max", function() return S:size("max") end)
  T.call("write_over", function() return S:write("over", big .. "y") end)
  T.call("size_over", function() return S:size("over") end)
  S:write("small", "old")
  T.call("overwrite_over", function() return S:write("small", big .. "y") end)
  T.call("small_unchanged", function() return S:read("small") == "old" end)
  T.call("max_unchanged", function() return S:read("max") == big end)
end
