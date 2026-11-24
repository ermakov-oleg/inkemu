local a, b, taps = nil, nil, 0

function on_load(ctx)
  a = ctx.layers:create(40, 40)
  a:draw(function(g) g:rect(0, 0, 40, 40, { fill = true }) end)
  b = ctx.layers:create(40, 40)
end

-- gray = true inside a layer's draw callback does not make the frame gray
function on_draw(ctx, g)
  g:clear(0)
  g:image(b, 10, 10)
end

-- layer:draw is refused inside on_draw, so it runs from the input callback; an error here means the gray call did not run
local function draw_gray_into_b()
  local ok, err = b:draw(function(lg)
    local _, e = lg:image(a, 0, 0, { gray = true })
    if e then error(e) end
  end)
  if err then error(err) end
end

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "up" then
    taps = taps + 1
    draw_gray_into_b()
    ctx:invalidate()
  end
  return true
end
