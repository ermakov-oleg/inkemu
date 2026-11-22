local T = require("conformance")

local methods = {
  { "get", "get" }, { "post", "post" }, { "Get", "Get" }, { "patch", "patch" }, { "put", "put" }, { "FOO", "FOO" },
}

function on_load(ctx)
  for _, m in ipairs(methods) do
    T.call("method_" .. m[1], function() return ctx.net:request({ url = "https://c.test/a", method = m[2] }, function() end) end)
  end
end
