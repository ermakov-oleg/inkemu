local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  T.value("path", F.path)
  F:write("same.txt", "file")
  T.call("storage_sees", function() return ctx.storage:exists("same.txt") end)
  ctx.storage:write("private.txt", "x")
  T.call("files_sees", function() return F:exists("private.txt") end)
end
