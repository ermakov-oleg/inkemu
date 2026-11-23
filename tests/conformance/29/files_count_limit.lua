local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  local failed = 0
  for i = 1, 512 do
    if F:write("f" .. i, "x") ~= true then failed = failed + 1 end
  end
  T.value("failed", failed)
  T.value("count", #F:list())
  T.call("write_new", function() return F:write("extra", "x") end)
  T.call("append_new", function() return F:append("extra", "x") end)
  T.call("exists_extra", function() return F:exists("extra") end)
  T.call("write_existing", function() return F:write("f1", "yy") end)
  T.call("append_existing", function() return F:append("f1", "z") end)
  T.call("read_f1", function() return F:read("f1") end)
  T.call("remove_one", function() return F:remove("f2") end)
  T.call("write_after_remove", function() return F:write("extra", "x") end)
end
