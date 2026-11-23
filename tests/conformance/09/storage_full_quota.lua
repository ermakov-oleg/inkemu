local T = require("conformance")

function on_load(ctx)
  local S = ctx.storage
  local full = string.rep("z", 262144)
  local failed = 0
  for i = 1, 512 do
    if S:write("f" .. i, full) ~= true then failed = failed + 1 end
  end
  T.value("failed", failed)
  T.value("usage_bytes", S:usage().bytes)
  T.value("usage_max_bytes", S:usage().max_bytes)
  T.call("overwrite_full", function() return S:write("f1", full) end)
  T.call("overwrite_smaller", function() return S:write("f2", "x") end)
  T.value("usage_after", S:usage().bytes)
end
