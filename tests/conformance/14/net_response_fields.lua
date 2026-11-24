local T = require("conformance")

function on_load(ctx)
  ctx.net:request({ url = "https://r.test/a" }, function(ctx, res)
    T.value("ok", res.ok)
    T.value("status", res.status)
    T.value("body", res.body)
    T.value("bytes", res.bytes)
    T.value("content_type", res.headers["content-type"])
    T.value("x_test", res.headers["x-test"])
    T.type("err", res.err)
    T.type("saved", res.saved)
  end)
end
