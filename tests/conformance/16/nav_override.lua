local T = require("conformance")
local ui

local function key(name)
  return ui:handle_event({ type = "key", key = name, phase = "down", time_ms = 0 })
end

-- `dir` from F (200, 100, 80 x 60, with `nav`), with D below it and `extra`
local function case(g, name, dir, nav, extra)
  ui:begin_frame(g)
  ui:button("F", { x = 200, y = 100, w = 80, h = 60, text = "F", nav = nav })
  ui:button("D", { x = 200, y = 200, w = 80, h = 60, text = "D" })
  if extra then extra() end
  ui:end_frame()
  ui:focus("F")
  T.call(name, function() return key(dir) end)
end

function on_load(ctx)
  ui = ctx.ui:create()
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then return end
  done = true
  local z = function(enabled)
    return function() ui:button("Z", { x = 400, y = 0, w = 50, h = 50, text = "Z", enabled = enabled }) end
  end
  case(g, "available", "down", { down = "Z" }, z())
  case(g, "against_the_direction", "down", { down = "Z" }, function()
    ui:button("Z", { x = 0, y = 0, w = 50, h = 50, text = "Z" })
  end)
  case(g, "itself", "down", { down = "F" })
  case(g, "disabled", "down", { down = "Z" }, z(false))
  case(g, "unknown", "down", { down = "nobody" })
  case(g, "label", "down", { down = "lab" }, function()
    ui:label("lab", { x = 0, y = 0, w = 50, h = 20, text = "L" })
  end)
  case(g, "other_direction", "right", { down = "Z" }, function()
    z()()
    ui:button("O", { x = 300, y = 100, w = 80, h = 60, text = "O" })
  end)
  case(g, "region_target", "up", { up = "R" }, function() ui:region("R", { x = 400, y = 400, w = 50, h = 50 }) end)
end
