local T = require("conformance")
local layer

function on_load(ctx)
  layer = ctx.layers:create(40, 40)
end

function on_draw(ctx, g)
  local function img(src) return g:image(layer, 0, 0, { src = src }) end
  T.call("src_too_big", function() return img({ x = 0, y = 0, w = 100, h = 100 }) end)
  T.call("src_overhang", function() return img({ x = 30, y = 30, w = 16, h = 16 }) end)
  T.call("src_negative_x", function() return img({ x = -1, y = 0, w = 16, h = 16 }) end)
  T.call("src_zero_w", function() return img({ x = 0, y = 0, w = 0, h = 16 }) end)
  T.call("src_fraction", function() return img({ x = 0.5, y = 0, w = 16, h = 16 }) end)
  T.call("src_positional_bad", function() return img({ 0, 0, 100, 100 }) end)
end
