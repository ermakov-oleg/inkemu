local T = require("conformance")

function on_load(ctx)
  ctx.files:write("dl.txt", "old content, longer than the answer")
  ctx.net:request({ url = "https://s.test/ok", save = { to = "files", name = "dl.txt" } }, function(ctx, res)
    T.value("ok", res.ok)
    T.value("status", res.status)
    T.value("saved", res.saved)
    T.value("body", res.body)
    T.value("bytes", res.bytes)
    T.type("err", res.err)
    T.value("file", ctx.files:read("dl.txt"))
  end)
end
