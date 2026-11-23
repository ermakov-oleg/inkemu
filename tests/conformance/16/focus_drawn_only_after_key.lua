local T = require("conformance")
local L = require("canvas_log")
local ui, keyless

local KEY = { type = "key", key = "back", phase = "up", time_ms = 0 }
local POINTER = { type = "pointer", phase = "down", x = 470, y = 790, time_ms = 0 }

-- whether the focused button `a` is drawn filled in a frame of `u`
local function drawn(u, g)
  u:begin_frame(g)
  u:button("a", { x = 20, y = 20, w = 200, h = 60, text = "A" })
  u:button("b", { x = 260, y = 20, w = 200, h = 60, text = "B" })
  local log = L.start(g)
  u:end_frame()
  return log[1] == L.call("rect", 20, 20, 200, 60, { fill = true, radius = 0 })
end

function on_load(ctx)
  ui = ctx.ui:create()
  -- inkemu-only lever: the library reads ctx.input.pointer at create, so false here gives an object with no touch
  local pointer = ctx.input.pointer
  ctx.input.pointer = false
  keyless = ctx.ui:create()
  ctx.input.pointer = pointer
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then
    drawn(ui, g)
    return
  end
  done = true
  T.value("touch", ctx.input.pointer)
  T.value("fresh", drawn(ui, g))
  ui:handle_event(KEY)
  T.value("after_key_up", drawn(ui, g))
  ui:handle_event(POINTER)
  T.value("after_pointer", drawn(ui, g))
  T.value("focus_kept", ui:focused())
  ui:handle_event({ type = "key", key = "zz", phase = "down", time_ms = 0 })
  T.value("after_unknown_key", drawn(ui, g))
  ui:handle_event({ type = "pointer", phase = "up", gesture = "swipe_left", x = 0, y = 0, down_x = 400, down_y = 0 })
  T.value("after_swipe", drawn(ui, g))
  ui:handle_event(KEY)
  ui:focus("b")
  ui:focus("a")
  T.value("focus_call_keeps_it_on", drawn(ui, g))
  ui:handle_event(POINTER)
  ui:focus("a")
  T.value("focus_call_keeps_it_off", drawn(ui, g))
  T.value("keyless.fresh", drawn(keyless, g))
  keyless:handle_event(POINTER)
  T.value("keyless.after_pointer", drawn(keyless, g))
  keyless:handle_event(KEY)
  ui:handle_event(KEY)
end
