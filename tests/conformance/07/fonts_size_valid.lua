local T = require("conformance")

local function got(ctx, px)
  local f, e = ctx.fonts:size(px)
  return f ~= nil, e
end

function on_load(ctx)
  T.call("size_8", function() return got(ctx, 8) end)
  T.call("size_96", function() return got(ctx, 96) end)
end
