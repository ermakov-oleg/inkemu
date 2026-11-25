local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

local function drawn(g, kind, o)
  ui:begin_frame(g)
  ui[kind](ui, "c", o)
  local log = L.start(g)
  ui:end_frame()
  return L.text(log)
end

local function join(parts)
  return table.concat(parts, " ")
end

local function scene(g, step_value, page)
  ui:begin_frame(g)
  ui:button("top", { x = 10, y = 20, w = 200, h = 56, text = "top" })
  ui:stepper("qty", { x = 10, y = 100, w = 200, h = 56, min = 1, max = 9, value = step_value, step = 3 })
  ui:pager("pg", { x = 250, y = 100, w = 200, h = 40, page = page, pages = 5 })
  ui:end_frame()
end

local function send(name, ev)
  T.call(name, function() return ui:handle_event(ev) end)
end

local function tap(x, y)
  return { type = "pointer", phase = "up", x = x, y = y, down_x = x, down_y = y, gesture = "tap", time_ms = 0 }
end

local function key(name, phase)
  return { type = "key", key = name, phase = phase or "down", time_ms = 0 }
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  local med = ctx.fonts.medium
  local function box(x, y, w, h, fill)
    return L.call("rect", x, y, w, h, { fill = fill == true, radius = 0 })
  end
  local function bar(x, y, w, h)
    return L.call("rect", x, y, w, h, { color = 15, fill = true })
  end
  local dots = function(x, y, w, h)
    return L.call("pattern", x, y, w, h, { color = 0, density = 50, kind = "dot" })
  end
  local function mid(x, y, w, h, s)
    return L.call("text_box", x, y, w, h, s, { align = "center", color = 15, font = med, overflow = "ellipsis",
      tabular = true, valign = "middle", wrap = "none" })
  end
  local function arrow(x, y, w, h, s)
    return L.call("text_box", x, y, w, h, s, { align = "center", color = 15, font = med, overflow = "ellipsis",
      valign = "middle", wrap = "none" })
  end
  -- 56 px parts: bars of 56 * 9 // 20 = 25 by 3, centred; the text takes what lies between the parts
  local stepper = join({
    box(10, 100, 56, 56), bar(26, 127, 25, 3),
    box(154, 100, 56, 56), bar(170, 127, 25, 3), bar(181, 116, 3, 25),
    mid(66, 100, 88, 56, "5"),
  })
  T.value("stepper", drawn(g, "stepper", { x = 10, y = 100, w = 200, h = 56, min = 1, max = 9, value = 5 }) == stepper)
  -- at an end, the part keeps its look and gets the dots over it right after its sign
  T.value("stepper_ends", drawn(g, "stepper", { x = 10, y = 100, w = 200, h = 56, min = 1, max = 9, value = 9,
    text = "max" }) == join({
    box(10, 100, 56, 56), bar(26, 127, 25, 3),
    box(154, 100, 56, 56), bar(170, 127, 25, 3), bar(181, 116, 3, 25), dots(154, 100, 56, 56),
    mid(66, 100, 88, 56, "max"),
  }))
  T.value("stepper_disabled", drawn(g, "stepper", { x = 10, y = 100, w = 200, h = 56, min = 1, max = 9, value = 5,
    enabled = false }) == stepper .. " " .. dots(10, 100, 200, 56))
  -- no room between the parts: the text is left out
  T.value("stepper_no_middle", drawn(g, "stepper", { x = 10, y = 100, w = 112, h = 56, min = 1, max = 9,
    value = 5 }) == join({
    box(10, 100, 56, 56), bar(26, 127, 25, 3),
    box(66, 100, 56, 56), bar(82, 127, 25, 3), bar(93, 116, 3, 25),
  }))
  T.value("pager", drawn(g, "pager", { x = 250, y = 100, w = 200, h = 40, page = 3, pages = 5 }) == join({
    box(250, 100, 40, 40), arrow(250, 100, 40, 40, "<"),
    box(410, 100, 40, 40), arrow(410, 100, 40, 40, ">"),
    mid(290, 100, 120, 40, "3/5"),
  }))
  T.value("pager_first_wide", drawn(g, "pager", { x = 250, y = 150, w = 200, h = 40, page = 1, pages = 5,
    button_w = 60 }) == join({
    box(250, 150, 60, 40), arrow(250, 150, 60, 40, "<"), dots(250, 150, 60, 40),
    box(390, 150, 60, 40), arrow(390, 150, 60, 40, ">"),
    mid(310, 150, 80, 40, "1/5"),
  }))
  T.call("stepper_short", function() return ui:stepper("c", { x = 10, y = 100, w = 100, h = 56, min = 1, max = 9,
    value = 5 }) end)
  T.call("stepper_range", function() return ui:stepper("c", { x = 10, y = 100, w = 200, h = 56, min = 5, max = 4,
    value = 5 }) end)
  T.call("stepper_step_zero", function() return ui:stepper("c", { x = 10, y = 100, w = 200, h = 56, min = 1,
    max = 9, value = 5, step = 0 }) end)
  T.call("pager_button_w", function() return ui:pager("c", { x = 250, y = 100, w = 200, h = 40, page = 3,
    pages = 5, button_w = 101 }) end)
  T.call("pager_page", function() return ui:pager("c", { x = 250, y = 100, w = 200, h = 40, page = 6, pages = 5 }) end)
  scene(g, 5, 3)
  if done then return end
  done = true
  send("tap_minus", tap(30, 128))
  send("tap_plus", tap(190, 128))
  send("tap_middle", tap(110, 128))
  send("tap_prev", tap(270, 120))
  send("tap_next", tap(430, 120))
  send("tap_pager_middle", tap(350, 120))
  send("hold_minus", { type = "pointer", phase = "hold", x = 30, y = 128, down_x = 30, down_y = 128, time_ms = 0 })
  -- at the ends the part is not hit
  scene(g, 9, 5)
  send("tap_plus_at_max", tap(190, 128))
  send("tap_next_on_last", tap(430, 120))
  send("tap_minus_clamped", tap(30, 128))
  scene(g, 2, 2)
  send("tap_minus_to_min", tap(30, 128))
  -- focus lands on the first available part, and the keys walk the parts
  scene(g, 1, 1)
  T.call("focus_qty", function() return ui:focus("qty") end)
  scene(g, 1, 1)
  T.call("focused_qty", function() return ui:focused() end)
  send("key_right", key("right"))
  send("key_ok_down", key("ok"))
  send("key_ok_up", key("ok", "up"))
end
