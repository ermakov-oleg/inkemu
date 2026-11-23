local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  local noop = function() end
  for _, ms in ipairs({ 999, 60001, 0, -1, 1.5 }) do
    T.call("timeout_" .. tostring(ms), function() return N:request({ url = "https://t.test/a", timeout_ms = ms }, noop) end)
  end
end
