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
    T.call("exists." .. label, function() return X:exists(name) end)
  end
end
