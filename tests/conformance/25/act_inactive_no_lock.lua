local T = require("conformance")

function on_load(ctx)
  T.call("load_sleep", function() return ctx.system:act("sleep") end)
  T.call("load_power_off", function() return ctx.system:act("power_off") end)
  ctx:after(100, function()
    T.call("timer_sleep", function() return ctx.system:act("sleep") end)
    T.call("timer_power_off", function() return ctx.system:act("power_off") end)
  end)
end

function on_input(ctx, ev)
  if ev.type == "pointer" and ev.phase == "down" then
    T.call("down_sleep", function() return ctx.system:act("sleep") end)
    T.call("down_power_off", function() return ctx.system:act("power_off") end)
  end
  return true
end
