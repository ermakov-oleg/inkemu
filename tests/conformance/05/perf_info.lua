local T = require("conformance")
local reported = false

function on_draw(ctx, g)
  g:rect(10, 10, 20, 20, { fill = true })
  g:rect(40, 10, 20, 20, { fill = true })
  g:rect(70, 10, 20, 20, { fill = true })
  if reported then return end
  reported = true
  ctx:after(100, function()
    local p = ctx.perf:info()
    for _, k in ipairs({ "tick_us", "draw_us", "cmd_count", "image_count", "asset_hit", "asset_miss", "draw_frame" }) do
      T.type("perf." .. k, p[k])
    end
    T.value("perf.cmd_count_value", p.cmd_count)
    T.value("perf.image_count_value", p.image_count)
    T.value("perf.drawn", type(p.draw_frame) == "number" and p.draw_frame >= 1)
  end)
end
