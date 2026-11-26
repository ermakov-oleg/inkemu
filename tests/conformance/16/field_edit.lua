local T = require("conformance")
local ui
local ends, mine, after_next = {}, nil, false

local function tap_at(x, y)
  return { type = "pointer", phase = "up", gesture = "tap", x = x, y = y, down_x = x, down_y = y, time_ms = 0 }
end

-- a field's on_end: `b` ends with Back here, and then the script opens its own edit and taps `a` while it is open
local function on_end(id)
  return function(ctx, text, how)
    ends[#ends + 1] = id .. ":" .. text .. ":" .. how
    T.value("ends", table.concat(ends, " "))
    after_next = id == "a"
    if id == "b" then
      mine = ctx.input:edit({ text = "mine" })
      T.call("field_while_script_edits", function() return ui:handle_event(tap_at(100, 60)) end)
      T.value("editing_while_script_edits", tostring(ui:editing()))
    end
    ctx:invalidate()
  end
end

function on_load(ctx)
  ui = ctx.ui:create()
end

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  ui:field("a", { x = 24, y = 40, w = 432, h = 56, text = "first", enter = "next", on_end = on_end("a") })
  ui:button("save", { x = 24, y = 110, w = 432, h = 56, text = "save" })
  ui:field("b", { x = 24, y = 180, w = 432, h = 56, text = "second", on_end = on_end("b") })
  ui:end_frame()
  if after_next then
    after_next = false
    T.value("after_next", tostring(ui:editing()) .. "/" .. tostring(ui:focused()))
  end
end

local taps = 0

function on_input(ctx, ev)
  local r = table.pack(ui:handle_event(ev))
  if r[1] == "save" and r[2] == "activate" then
    T.call("save", function() return ui:stop_edit() end)
    T.value("editing_after_save", tostring(ui:editing()))
  end
  if r[2] == "edit" then
    taps = taps + 1
    T.value("tap" .. taps, { r[1], r[2], ui:editing() })
    if taps == 1 then
      T.call("script_edit_while_field_edits", function() return ctx.input:edit({ text = "x" }) end)
    elseif taps == 2 then
      ctx:after(500, function()
        T.call("stop_edit", function() return ui:stop_edit() end)
        T.value("editing_after_stop", tostring(ui:editing()))
      end)
    end
  end
  if r[2] then ctx:invalidate() end
  return r[2] ~= nil
end
