local T = require("conformance")

local cases = {
  { "upper", "Bad" },
  { "dot_start", ".x" },
  { "dot_end", "x." },
  { "long65", string.rep("a", 65) },
}

function on_load(ctx)
  local X = ctx.storage
  for _, case in ipairs(cases) do
    local label, name = case[1], case[2]
    T.call("write." .. label, function() return X:write(name, "x") end)
    T.call("read." .. label, function() return X:read(name) end)
    T.call("size." .. label, function() return X:size(name) end)
    T.call("remove." .. label, function() return X:remove(name) end)
  end
end
