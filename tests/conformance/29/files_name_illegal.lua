local T = require("conformance")

local cases = {
  { "dot_start", ".hidden" },
  { "colon", "a:b" },
  { "star", "a*b" },
  { "question", "a?b" },
  { "quote", "a\"b" },
  { "lt", "a<b" },
  { "gt", "a>b" },
  { "pipe", "a|b" },
  { "tilde", "a~b" },
  { "dot_end", "x." },
  { "long129", string.rep("a", 129) },
  { "long_utf8", string.rep("\u{44f}", 65) },
  { "con", "CON" },
  { "con_ext", "con.txt" },
  { "nul_mixed", "Nul" },
  { "com1", "COM1" },
  { "lpt9_ext", "LPT9.log" },
  { "aux", "aux" },
}

function on_load(ctx)
  local X = ctx.files
  for _, case in ipairs(cases) do
    local label, name = case[1], case[2]
    T.call("write." .. label, function() return X:write(name, "x") end)
    T.call("exists." .. label, function() return X:exists(name) end)
  end
end
