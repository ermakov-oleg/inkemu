local T = require("conformance")

local illegal = { dot_start = ".hidden", slash = "a/b", con = "CON" }

function on_load(ctx)
  local F = ctx.files
  for label, name in pairs(illegal) do
    T.call("read." .. label, function() return F:read(name) end)
    T.call("size." .. label, function() return F:size(name) end)
    T.call("remove." .. label, function() return F:remove(name) end)
    T.call("append." .. label, function() return F:append(name, "x") end)
  end
end
