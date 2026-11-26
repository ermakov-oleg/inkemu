local T = require("conformance")

local got, ends = {}, {}

local function on_end(ctx, text, how)
  ends[#ends + 1] = text .. ":" .. how
  T.value("ends", table.concat(ends, " "))
  -- a new edit may start in on_end; the next one is ended with the ok key
  if how == "back" then
    ctx.input:edit({ text = "again", on_end = on_end })
  end
end

function on_load(ctx)
  ctx.input:edit({ text = "hi", rect = { x = 32, y = 132, w = 416, h = 40 }, on_end = on_end })
end

function on_input(ctx, ev)
  if ev.type == "key" then
    got[#got + 1] = "key." .. ev.key .. ":" .. ev.phase
  else
    got[#got + 1] = ev.phase .. ":" .. tostring(ev.gesture)
  end
  T.value("got", table.concat(got, " "))
  return true
end

function on_draw(ctx, g)
  g:clear(0)
end
