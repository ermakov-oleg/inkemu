-- the first tap draws a gray frame, later taps normal frames
local layer, taps = nil, 0

function on_load(ctx)
  layer = ctx.layers:create(40, 40)
  layer:draw(function(g) g:rect(0, 0, 40, 40, { fill = true }) end)
end

function on_draw(ctx, g)
  g:clear(0)
  g:image(layer, 10, 10, { gray = taps == 1 })
end

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "up" then
    taps = taps + 1
    ctx:invalidate()
  end
  return true
end
