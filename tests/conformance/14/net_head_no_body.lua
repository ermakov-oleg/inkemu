local T = require("conformance")

function on_load(ctx)
  ctx.net:request({ url = "https://h.test/h", method = "HEAD" }, function(ctx, res)
    T.value("status", res.status)
    T.value("body", res.body)
    T.value("bytes", res.bytes)
    T.value("header_a", res.headers["x-a"])
  end)
end
