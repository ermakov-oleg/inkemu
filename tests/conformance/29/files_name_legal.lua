local T = require("conformance")

local names = {
  book = "Moon-Phases.txt", inner_space = "a b.txt", utf8 = "\u{43a}\u{43d}\u{438}\u{433}\u{430}.txt",
  long128 = string.rep("a", 128), long_utf8_128 = string.rep("\u{44f}", 64), prefix_con = "console.txt",
  conx = "CONX", multi_dot = "a.b.c", dash = "-x",
}

function on_load(ctx)
  local F = ctx.files
  for label, name in pairs(names) do
    T.call("write." .. label, function() return F:write(name, "x") end)
    T.call("exists." .. label, function() return F:exists(name) end)
  end
end
