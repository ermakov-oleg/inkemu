local T = require("conformance")

local cases = {
  { "slash", "a/b" },
  { "backslash", "a\\b" },
  { "ctrl", "a\1b" },
  { "del", "a\127b" },
  { "empty", "" },
  { "space_end", "x " },
}

function on_load(ctx)
  local X = ctx.files
  for _, case in ipairs(cases) do
    local label, name = case[1], case[2]
    T.call("write." .. label, function() return X:write(name, "x") end)
    T.call("exists." .. label, function() return X:exists(name) end)
  end
end
