local T = require("conformance")

function on_load(ctx)
  ctx.net:request({ url = "https://cap.test/exact" }, function(ctx, res)
    T.value("exact_len", #res.body)
    T.value("exact_bytes", res.bytes)
    T.type("exact_err", res.err)
  end)
end
