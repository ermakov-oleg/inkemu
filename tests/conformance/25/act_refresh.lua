local T = require("conformance")

function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "up" then
    T.call("act_refresh", function() return ctx.system:act("refresh") end)
    ctx:invalidate()
  end
  return true
end
