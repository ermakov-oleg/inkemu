local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  local noop = function() end
  for i, ms in ipairs({ 999, 60001, 0, -1, 1.5 }) do
    T.call("bad_" .. i, function() return N:request({ url = "https://t.test/a", timeout_ms = ms }, noop) end)
  end
  local min = N:request({ url = "https://t.test/a", timeout_ms = 1000 }, function(ctx, res)
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
