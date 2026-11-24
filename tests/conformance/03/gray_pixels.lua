local layer, gray = nil, false

function on_load(ctx)
  layer = ctx.layers:create(40, 40)
  layer:draw(function(g) g:rect(0, 0, 40, 40, { fill = true }) end)
end

-- colors 3, 4, 8, 12, 15 give 0, 1, 2, 3, 3 ink steps on a gray frame
function on_draw(ctx, g)
  g:clear(0)
  for i, c in ipairs({ 3, 4, 8, 12, 15 }) do
    g:rect(10 + (i - 1) * 30, 10, 20, 20, { fill = true, color = c })
  end
  g:image(layer, 10, 100, { gray = gray })
end

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "up" then
    gray = true
    ctx:invalidate()
  end
  return true
end
