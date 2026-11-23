local T = require("conformance")
local layer, scheduled = nil, false

function on_load(ctx)
  layer = ctx.layers:create(40, 40)
  T.call("release", function() return layer:release() end)
  T.call("release_again", function() return layer:release() end)
  T.call("draw_released", function() return layer:draw(function(g) end) end)
end

function on_draw(ctx, g)
  T.call("image_released", function() return g:image(layer, 0, 0) end)
end
