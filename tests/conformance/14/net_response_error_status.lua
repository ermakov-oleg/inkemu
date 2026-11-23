local T = require("conformance")

function on_load(ctx)
  ctx.net:request({ url = "https://r.test/gone" }, function(ctx, res)
    T.value("ok", res.ok)
    T.value("status", res.status)
    T.value("body", res.body)
    T.type("err", res.err)
  end)
end
