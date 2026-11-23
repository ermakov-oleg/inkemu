local T = require("conformance")

local function usage(ctx, label)
  local u = ctx.storage:usage()
  T.type(label, u)
  T.value(label .. ".files", u.files)
  T.value(label .. ".bytes", u.bytes)
  T.value(label .. ".max_files", u.max_files)
  T.value(label .. ".max_bytes", u.max_bytes)
end

function on_load(ctx)
  usage(ctx, "before")
  T.call("write", function() return ctx.storage:write("note.txt", "hello") end)
  usage(ctx, "after")
end
