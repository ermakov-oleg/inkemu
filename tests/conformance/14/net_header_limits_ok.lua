local T = require("conformance")

local function cases()
  local sixteen = {}
  for i = 1, 16 do sixteen["h" .. i] = "v" end
  return {
    { "sixteen", sixteen },
    { "name64", { [string.rep("n", 64)] = "v" } },
    { "value_8193", { x = string.rep("v", 8193) } },
    { "total_65536", { x = string.rep("v", 65535) } },
  }
end

function on_load(ctx)
  for i, case in ipairs(cases()) do
    ctx:after(i * 1000, function(ctx)
      local job = ctx.net:request({ url = "https://hd.test/a", headers = case[2] }, function(ctx, res)
        T.value("status_" .. case[1], res.status)
      end)
      T.value("started_" .. case[1], job ~= nil)
    end)
  end
end
