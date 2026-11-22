local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  F:write("d", "0123456789")
  T.call("whole", function() return F:read("d") end)
  T.call("middle", function() return F:read("d", 2, 3) end)
  T.call("tail", function() return F:read("d", 8, 5) end)
  T.call("at_end", function() return F:read("d", 10, 4) end)
  T.call("past_end", function() return F:read("d", 50, 4) end)
  T.call("offset_only", function() return F:read("d", 7) end)
  T.call("one_byte", function() return F:read("d", 0, 1) end)
  T.call("missing", function() return F:read("nothing", 0, 4) end)
end
