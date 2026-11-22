local T = require("conformance")

local cases = {
  { "inner_space", "a b.txt" },
}

function on_load(ctx)
  local F = ctx.files
  for _, case in ipairs(cases) do
    local label, name = case[1], case[2]
    T.call("write." .. label, function() return F:write(name, "x") end)
    T.call("exists." .. label, function() return F:exists(name) end)
  end
end
