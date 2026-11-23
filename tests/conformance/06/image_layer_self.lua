local T = require("conformance")

function on_load(ctx)
  local layer = ctx.layers:create(40, 40)
  layer:draw(function(g)
    T.call("self_draw", function() return g:image(layer, 0, 0) end)
  end)
end
