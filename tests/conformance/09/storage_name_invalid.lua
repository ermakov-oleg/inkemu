local T = require("conformance")

local bad = {
  upper = "Bad", dot_start = ".x", dot_end = "x.", empty = "",
  long65 = string.rep("a", 65), space = "a b", slash = "a/b",
}

function on_load(ctx)
  local S = ctx.storage
  for label, name in pairs(bad) do
    T.call("write." .. label, function() return S:write(name, "x") end)
    T.call("read." .. label, function() return S:read(name) end)
    T.call("size." .. label, function() return S:size(name) end)
    T.call("remove." .. label, function() return S:remove(name) end)
    T.call("exists." .. label, function() return S:exists(name) end)
  end
end
