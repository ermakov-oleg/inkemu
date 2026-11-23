local T = require("conformance")

local methods = {
  "GET",
  "POST",
}

function on_load(ctx)
  -- one request per second: a method the core never answers must not hide the others
  for i, m in ipairs(methods) do
    ctx:after(i * 1000, function(ctx)
      local job = ctx.net:request({ url = "https://m.test/" .. m, method = m }, function(ctx, res)
        T.value("status_" .. m, res.status)
        T.value("header_" .. m, res.headers["x-method"])
      end)
      T.value("started_" .. m, job ~= nil)
    end)
  end
end
