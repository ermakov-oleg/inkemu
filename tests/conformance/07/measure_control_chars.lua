local T = require("conformance")

function on_load(ctx)
  local f = ctx.fonts.medium
  T.call("tab_is_space", function() return f:measure("a\tb") == f:measure("a b") end)
  T.call("newline_zero_width", function() return f:measure("a\nb") == f:measure("ab") end)
  T.call("control_zero_width", function() return f:measure("a\1b") == f:measure("ab") end)
end
