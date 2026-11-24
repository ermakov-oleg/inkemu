local T = require("conformance")

function on_load(ctx)
  ctx.files:write("keep.txt", "keep")
  ctx.net:request({ url = "https://s.test/fail", save = { to = "files", name = "keep.txt" } }, function(ctx, res)
    T.value("ok", res.ok)
    T.value("status", res.status)
    T.value("saved", res.saved)
    T.value("body", res.body)
    T.type("err", res.err)
    T.value("file", ctx.files:read("keep.txt"))
  end)
end
