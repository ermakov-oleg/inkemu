local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  F:write("d", "0123456789")
  T.call("negative_offset", function() return F:read("d", -1, 4) end)
  T.call("len_zero", function() return F:read("d", 0, 0) end)
  T.call("len_negative", function() return F:read("d", 0, -5) end)
  T.call("len_over", function() return F:read("d", 0, 65537) end)
  T.call("len_way_over", function() return F:read("d", 0, 1000000) end)
end
