function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end

local first = true

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "up" then
    if first then
      ctx:invalidate("full")
      ctx:defer_full_refresh(60000)
      first = false
    else
      ctx:invalidate()
    end
  end
  return true
end
