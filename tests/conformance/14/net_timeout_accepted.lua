local T = require("conformance")

function on_load(ctx)
  local min = ctx.net:request({ url = "https://t.test/a", timeout_ms = 1000 }, function(ctx, res)
    T.value("min_status", res.status)
    ctx:after(1000, function(ctx)
      local max = ctx.net:request({ url = "https://t.test/a", timeout_ms = 60000 }, function(ctx, res)
        T.value("max_status", res.status)
      end)
      T.value("max_started", max ~= nil)
    end)
  end)
  T.value("min_started", min ~= nil)
end
