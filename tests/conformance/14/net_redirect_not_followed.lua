local T = require("conformance")

function on_load(ctx)
  ctx.net:request({ url = "https://r.test/old" }, function(ctx, res)
    T.value("status", res.status)
    T.value("location", res.headers["location"])
    T.value("body", res.body)
  end)
end
