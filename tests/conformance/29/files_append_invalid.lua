local T = require("conformance")

function on_load(ctx)
  local F = ctx.files
  F:append("log", "abc")
  T.call("append_bad_name", function() return F:append(".log", "x") end)
  T.call("append_over", function() return F:append("log", string.rep("x", 262145)) end)
  T.call("unchanged", function() return F:read("log") == "abc" end)
end
