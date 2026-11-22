local a, b, taps = nil, nil, 0

function on_load(ctx)
  a = ctx.layers:create(40, 40)
  a:draw(function(g) g:rect(0, 0, 40, 40, { fill = true }) end)
  b = ctx.layers:create(40, 40)
end

-- gray = true inside a layer's draw callback does not make the frame gray
function on_draw(ctx, g)
  g:clear(0)
  if taps == 1 then b:draw(function(lg) lg:image(a, 0, 0, { gray = true }) end) end
  g:image(b, 10, 10)
end

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "up" then
    taps = taps + 1
    ctx:invalidate()
  end
  return true
end
