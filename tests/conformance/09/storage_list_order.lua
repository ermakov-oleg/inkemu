local T = require("conformance")

local function copy(list)
  local out = {}
  for i, name in ipairs(list) do out[i] = name end
  return out
end

function on_load(ctx)
  local S = ctx.storage
  T.call("list_empty", function() return copy(S:list()) end)
  for _, name in ipairs({ "b", "a", "a0", "a_", "a.b", "a-1", "0z" }) do S:write(name, "x") end
  T.call("list", function() return copy(S:list()) end)
end
