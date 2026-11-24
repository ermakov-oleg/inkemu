local T = require("conformance")
local ui

local function key(name)
  return ui:handle_event({ type = "key", key = name, phase = "down", time_ms = 0 })
end

local function walk(keys)
  local out = {}
  for k in string.gmatch(keys, "%a+") do
    local id, action = key(k)
    out[#out + 1] = tostring(id) .. "," .. tostring(action)
  end
  return table.concat(out, " ")
end

function on_load(ctx)
  ui = ctx.ui:create()
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then return end
  done = true
  -- A B above, C below A: by position down from A would be C
  ui:begin_frame(g)
  ui:button("A", { x = 20, y = 20, w = 100, h = 60, text = "A" })
  ui:button("B", { x = 200, y = 20, w = 100, h = 60, text = "B" })
  ui:button("C", { x = 20, y = 200, w = 100, h = 60, text = "C" })
  ui:end_frame()
  T.value("declaration_order", walk("down down up left right"))
  ui:begin_frame(g)
  ui:button("m1", { x = 0, y = 0, w = 50, h = 50, text = "1", nav = { right = "m4" } })
  ui:button("m2", { x = 60, y = 0, w = 50, h = 50, text = "2", enabled = false })
  ui:region("m3", { x = 120, y = 0, w = 50, h = 50 })
  ui:button("m4", { x = 180, y = 0, w = 50, h = 50, text = "4" })
  ui:end_frame()
  ui:focus("m1")
  T.value("skips_disabled_ignores_nav", walk("right right"))
  T.call("past_the_last", function() return key("down") end)
  ui:focus("m1")
  T.call("before_the_first", function() return key("up") end)
  T.value("focused", ui:focused())
end
