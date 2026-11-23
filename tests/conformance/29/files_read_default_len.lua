local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  F:write("f", string.rep("y", 70000))
  T.call("default_len", function() return #F:read("f") end)
  T.call("second_chunk", function() return #F:read("f", 65536) end)
  T.call("ten", function() return #F:read("f", 0, 10) end)
  T.call("max_len", function() return #F:read("f", 0, 65536) end)
end
