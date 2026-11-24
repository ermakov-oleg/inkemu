local T = require("conformance")

function on_load(ctx)
  T.value("after_ran", ctx.storage:read("after") ~= nil)
  ctx:after(100, function(ctx) ctx.storage:write("after", "1") end)
end

function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end
