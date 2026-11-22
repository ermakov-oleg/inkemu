local draws = 0

function on_draw(ctx, g)
  draws = draws + 1
  g:rect(10, 10, 20, 20, { fill = true })
  if draws == 2 then ctx:invalidate("full") end
end

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "up" then ctx:invalidate() end
  return true
end
