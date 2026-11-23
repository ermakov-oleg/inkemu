local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  local first = N:request({ url = "https://b.test/slow" }, function(ctx, res)
    T.value("first_status", res.status)
  end)
  T.value("first_started", first ~= nil)
  T.call("second_get", function() return N:request({ url = "https://b.test/slow" }, function() end) end)
  T.call("bad_method_not_busy", function() return N:request({ url = "https://b.test/slow", method = "FOO" }, function() end) end)
  ctx:after(2000, function(ctx)
    local again = ctx.net:request({ url = "https://b.test/slow" }, function(ctx, res)
      T.value("again_status", res.status)
    end)
    T.value("again_started", again ~= nil)
  end)
end
