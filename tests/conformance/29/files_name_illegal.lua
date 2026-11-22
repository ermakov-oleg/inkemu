local T = require("conformance")

local illegal = {
  dot_start = ".hidden",
  slash = "a/b",
  backslash = "a\\b",
  colon = "a:b",
  star = "a*b",
  question = "a?b",
  quote = "a\"b",
  lt = "a<b",
  gt = "a>b",
  pipe = "a|b",
  tilde = "a~b",
  dot_end = "x.",
  space_end = "x ",
  ctrl = "a\1b",
  del = "a\127b",
  empty = "",
  long129 = string.rep("a", 129),
  long_utf8 = string.rep("\u{44f}", 65),
  con = "CON",
  con_ext = "con.txt",
  nul_mixed = "Nul",
  com1 = "COM1",
  lpt9_ext = "LPT9.log",
  aux = "aux",
}

function on_load(ctx)
  local F = ctx.files
  for label, name in pairs(illegal) do
    T.call("write." .. label, function() return F:write(name, "x") end)
    T.call("exists." .. label, function() return F:exists(name) end)
  end
end
