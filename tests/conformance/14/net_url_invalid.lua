local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  local noop = function() end
  local base = "https://u.test/"
  local cases = {
    ftp = "ftp://u.test/a",
    no_scheme = "u.test/a",
    ws = "ws://u.test/a",
    empty = "",
    long1025 = base .. string.rep("a", 1025 - #base),
  }
  for label, url in pairs(cases) do
    T.call(label, function() return N:request({ url = url }, noop) end)
  end
end
