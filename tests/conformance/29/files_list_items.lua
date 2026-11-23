local T = require("conformance")

local function items(list)
  local out = {}
  for i, e in ipairs(list) do out[i] = { name = e.name, size = e.size } end
  return out
end

function on_load(ctx)
  local F = ctx.files
  F:write("b.txt", "bb")
  F:write("A.txt", "a")
  F:write("a1.txt", "")
  F:write("Z.txt", "zzz")
  T.call("list", function() return items(F:list()) end)
end
