local T = require("conformance")
local ui

local function pointer(phase, x, y, dx, dy, gesture)
  return { type = "pointer", phase = phase, x = x, y = y, down_x = dx, down_y = dy, gesture = gesture, time_ms = 0 }
end

local function send(name, ev)
  T.call(name, function() return ui:handle_event(ev) end)
  T.value(name .. ".focused", ui:focused())
end

function on_load(ctx)
  ui = ctx.ui:create()
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  ui:button("a", { x = 20, y = 20, w = 200, h = 60, text = "A" })
  ui:button("b", { x = 260, y = 20, w = 200, h = 60, text = "B" })
  ui:end_frame()
  if done then return end
  done = true
  send("tap", pointer("up", 300, 40, 300, 40, "tap"))
  send("double_tap", pointer("up", 30, 40, 30, 40, "double_tap"))
  send("hold", pointer("hold", 300, 40, 300, 40))
  send("press_point", pointer("up", 300, 40, 30, 40, "tap"))
  send("press_point_empty", pointer("up", 300, 40, 30, 700, "tap"))
  send("hold_press_point", pointer("hold", 30, 700, 300, 40))
  send("no_down_point", pointer("up", 300, 40, nil, nil, "tap"))
  send("down", pointer("down", 30, 40))
  send("move", pointer("move", 30, 40, 30, 40))
  send("swipe", pointer("up", 200, 40, 30, 40, "swipe_right"))
  send("long", pointer("up", 30, 40, 30, 40, "long"))
  send("cancel", pointer("cancel", 30, 40))
  send("no_point", { type = "pointer", phase = "up", gesture = "tap" })
end

-- the runner's tap reaches handle_event as a pointer down and an up with gesture tap
local seen = {}

function on_input(ctx, ev)
  local r = table.pack(ui:handle_event(ev))
  seen[#seen + 1] = ev.phase .. ":" .. tostring(r[1]) .. "," .. tostring(r[2])
  T.value("runner_tap", table.concat(seen, " "))
  return r[2] ~= nil
end
