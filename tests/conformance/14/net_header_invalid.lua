local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  local noop = function() end
  local many = {}
  for i = 1, 17 do many["h" .. i] = "v" end
  local cases = {
    cr = { ["x-a"] = "a\rb" },
    lf = { ["x-a"] = "a\nb" },
    nul = { ["x-a"] = "a\0b" },
    seventeen = many,
    name65 = { [string.rep("n", 65)] = "v" },
    total = { x = string.rep("v", 65536) },
  }
  for label, headers in pairs(cases) do
    T.call(label, function() return N:request({ url = "https://hd.test/a", headers = headers }, noop) end)
  end
end
