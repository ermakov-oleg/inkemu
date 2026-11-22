local T = require("conformance")

function on_load(ctx)
  for i = 1, 512 do ctx.files:write("f" .. i, "x") end
  ctx:after(1000, function(ctx)
    ctx.net:request({ url = "https://s.test/ok", save = { to = "files", name = "new.txt" } }, function(ctx, res)
      T.value("new_saved", res.saved)
      T.value("new_err", res.err)
      T.value("new_exists", ctx.files:exists("new.txt"))
    end)
  end)
  ctx:after(3000, function(ctx)
    ctx.net:request({ url = "https://s.test/ok", save = { to = "files", name = "f1" } }, function(ctx, res)
      T.value("existing_saved", res.saved)
      T.value("existing_file", ctx.files:read("f1"))
    end)
  end)
end
