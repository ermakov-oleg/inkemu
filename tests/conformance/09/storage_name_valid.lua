local T = require("conformance")

local names = {
  one = "a", digit = "0", mixed = "a.b_c-1", long64 = string.rep("a", 64), leading_dash = "-x", ext = "notes.txt",
}

function on_load(ctx)
  local S = ctx.storage
  for label, name in pairs(names) do
    T.call("write." .. label, function() return S:write(name, "x") end)
    T.call("exists." .. label, function() return S:exists(name) end)
  end
end
