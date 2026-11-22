-- the first session draws a gray frame, every later one a normal frame
local layer
local first = false

function on_load(ctx)
  first = ctx.storage:read("seen") == nil
  ctx.storage:write("seen", "1")
  layer = ctx.layers:create(40, 40)
  layer:draw(function(g) g:rect(0, 0, 40, 40, { fill = true }) end)
end

function on_draw(ctx, g)
  g:clear(0)
  g:image(layer, 10, 10, { gray = first })
end
