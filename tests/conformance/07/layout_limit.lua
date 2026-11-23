local T = require("conformance")

function on_load(ctx)
  local f = ctx.fonts.small
  T.call("layout_65536", function()
    local lines, height = f:layout(string.rep("a", 65536), 400, 800)
    return type(lines), type(height)
  end)
  T.call("layout_65537", function() return f:layout(string.rep("a", 65537), 400, 800) end)
end
