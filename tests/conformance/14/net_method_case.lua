local T = require("conformance")

local methods = { "get", "post", "Get", "patch", "put", "FOO", "" }

function on_load(ctx)
  for i, m in ipairs(methods) do
    T.call("method_" .. i, function() return ctx.net:request({ url = "https://c.test/a", method = m }, function() end) end)
  end
end
