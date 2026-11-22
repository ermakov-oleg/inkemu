local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  local noop = function() end
  local base = "https://u.test/"
  local cases = {
    { "ftp", "ftp://u.test/a" },
    { "no_scheme", "u.test/a" },
    { "ws", "ws://u.test/a" },
    { "empty", "" },
  }
  for _, case in ipairs(cases) do
    T.call(case[1], function() return N:request({ url = case[2] }, noop) end)
  end
end
