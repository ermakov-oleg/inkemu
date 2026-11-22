local T = require("conformance")

function on_load(ctx)
  local S = ctx.storage
  local failed = 0
  for i = 1, 512 do
    if S:write("f" .. i, "x") ~= true then failed = failed + 1 end
  end
  T.value("failed", failed)
  T.value("usage_files", S:usage().files)
  T.call("write_513", function() return S:write("extra", "x") end)
  T.call("exists_extra", function() return S:exists("extra") end)
  T.call("overwrite_existing", function() return S:write("f1", "yy") end)
  T.call("size_f1", function() return S:size("f1") end)
  T.call("remove_one", function() return S:remove("f2") end)
  T.call("write_after_remove", function() return S:write("extra", "x") end)
end
