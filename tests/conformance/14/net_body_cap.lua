local T = require("conformance")

function on_load(ctx)
  ctx:after(1000, function(ctx)
    ctx.net:request({ url = "https://cap.test/big" }, function(ctx, res)
      T.value("big_ok", res.ok)
      T.value("big_status", res.status)
      T.value("big_len", #res.body)
      T.value("big_bytes", res.bytes)
      T.value("big_err", res.err)
    end)
  end)
end
