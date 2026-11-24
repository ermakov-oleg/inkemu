local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  local full = string.rep("z", 262144)
  local failed = 0
  for i = 1, 512 do
    if F:write("f" .. i, full) ~= true then failed = failed + 1 end
  end
  T.value("failed", failed)
  T.call("append_existing", function() return F:append("f1", "x") end)
  T.call("size_f1", function() return F:size("f1") end)
  T.call("write_existing", function() return F:write("f2", full) end)
end
