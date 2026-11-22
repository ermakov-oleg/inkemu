local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  T.call("read_missing", function() return F:read("nothing") end)
  T.call("size_missing", function() return F:size("nothing") end)
  T.call("exists_missing", function() return F:exists("nothing") end)
end
