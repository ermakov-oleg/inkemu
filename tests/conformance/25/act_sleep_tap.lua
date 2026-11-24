local T = require("conformance")

function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "up" then
    T.call("sleep", function() return ctx.system:act("sleep") end)
  end
  return true
end

function on_leave(ctx) T.value("left", true) end
