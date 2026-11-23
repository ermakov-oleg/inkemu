local T = require("conformance")
local layer

function on_load(ctx)
  layer = ctx.layers:create(40, 40)
end

function on_draw(ctx, g)
  local function img(src) return g:image(layer, 0, 0, { src = src }) end
  T.call("src_named_ok", function() return img({ x = 0, y = 0, w = 16, h = 16 }) end)
  T.call("src_positional_ok", function() return img({ 0, 0, 16, 16 }) end)
  T.call("src_edge_ok", function() return img({ x = 24, y = 24, w = 16, h = 16 }) end)
end
