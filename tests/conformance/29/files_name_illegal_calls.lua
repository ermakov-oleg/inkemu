local T = require("conformance")

local cases = {
  { "dot_start", ".hidden" },
  { "con", "CON" },
}

function on_load(ctx)
  local X = ctx.files
  for _, case in ipairs(cases) do
    local label, name = case[1], case[2]
    T.call("read." .. label, function() return X:read(name) end)
    T.call("size." .. label, function() return X:size(name) end)
    T.call("remove." .. label, function() return X:remove(name) end)
    T.call("append." .. label, function() return X:append(name, "x") end)
  end
end
