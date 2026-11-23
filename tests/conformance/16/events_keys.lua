local T = require("conformance")
local ui

local function key(name, phase, extra)
  local ev = { type = "key", key = name, phase = phase, time_ms = 0 }
  for k, v in pairs(extra or {}) do ev[k] = v end
  return ev
end

local function frame(g)
  ui:begin_frame(g)
  ui:button("a", { x = 20, y = 20, w = 200, h = 60, text = "A" })
  ui:button("b", { x = 260, y = 20, w = 200, h = 60, text = "B" })
  ui:button("off", { x = 20, y = 100, w = 200, h = 60, text = "X", enabled = false })
  ui:end_frame()
end

local function send(name, ev)
  T.call(name, function() return ui:handle_event(ev) end)
end

function on_load(ctx)
  ui = ctx.ui:create()
  send("before_any_frame", key("ok", "down"))
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  frame(g)
  if done then return end
  done = true
  send("ok_down", key("ok", "down"))
  send("ok_up", key("ok", "up"))
  send("ok_up_again", key("ok", "up"))
  send("ok_down_2", key("ok", "down"))
  frame(g)
  send("ok_up_after_frame", key("ok", "up"))
  send("ok_down_3", key("ok", "down"))
  send("arrow_away", key("right", "down"))
  send("ok_up_after_move", key("ok", "up"))
  send("arrow_up_phase", key("left", "up"))
  send("back", key("back", "down"))
  send("unknown_key", key("menu", "down"))
  send("no_key", { type = "key", phase = "down" })
  send("gamepad", key("ok", "down", { source = "gamepad", button = "a" }))
  send("repeat_arrow", key("down", "repeat"))
  send("repeat_ok", key("ok", "repeat"))
  send("no_phase", { type = "key", key = "ok" })
  send("not_a_table", "ok")
  send("nil", nil)
  send("empty", {})
  send("other_type", { type = "timer", phase = "down" })
  T.value("focused", ui:focused())
end

-- the runner's own key events reach handle_event the same way
local seen = {}

function on_input(ctx, ev)
  local r = table.pack(ui:handle_event(ev))
  seen[#seen + 1] = ev.phase .. ":" .. tostring(r[1]) .. "," .. tostring(r[2])
  T.value("runner_ok", table.concat(seen, " "))
  return r[2] ~= nil
end
