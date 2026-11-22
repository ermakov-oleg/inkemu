local T = require("conformance")

-- the sessions share storage, so the sequence of lock.net values builds up across wakes
function on_load(ctx)
  local seq = ctx.storage:read("seq")
  seq = (seq and seq .. "," or "") .. tostring(ctx.lock.net)
  ctx.storage:write("seq", seq)
  T.value("net_seq", seq)
end

function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end
