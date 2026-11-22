local T = require("conformance")

local function got(ctx, px)
  local f, e = ctx.fonts:size(px)
  return f ~= nil, e
end

function on_load(ctx)
  T.call("size_8", function() return got(ctx, 8) end)
  T.call("size_96", function() return got(ctx, 96) end)
  T.call("size_7", function() return ctx.fonts:size(7) end)
  T.call("size_97", function() return ctx.fonts:size(97) end)
  T.call("size_0", function() return ctx.fonts:size(0) end)
  T.call("size_fraction", function() return ctx.fonts:size(24.5) end)
end
