function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end

local full = false

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "up" then
    if full then ctx:invalidate() else ctx:invalidate("full") end
    full = true
  end
  return true
end
