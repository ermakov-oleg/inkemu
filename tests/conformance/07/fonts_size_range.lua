local T = require("conformance")

function on_load(ctx)
  T.call("size_7", function() return ctx.fonts:size(7) end)
  T.call("size_97", function() return ctx.fonts:size(97) end)
  T.call("size_0", function() return ctx.fonts:size(0) end)
  T.call("size_fraction", function() return ctx.fonts:size(24.5) end)
end
