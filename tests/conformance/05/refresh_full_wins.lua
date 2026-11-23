function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "up" then
    ctx:invalidate()
    ctx:invalidate("full")
    ctx:invalidate()
  end
  return true
end
