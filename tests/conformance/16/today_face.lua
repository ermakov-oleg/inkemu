local T = require("conformance")
local B = require("layout_box")
local L = require("canvas_log")
local ui, fonts

-- A picker page in the shape of the "Today" app's clock-face screen: a back region and a large bold title, an empty
-- bordered container whose box the app reads back from ui:layout to draw a preview in, a three-item radio and a hint.
local HINT = "Arrows move the preview, a tap or OK keeps the choice for the next lock."
local ITEMS = { "Matrix", "Digits", "Dial" }

local function tree(value)
  return { kind = "column", padding = 16, gap = 16, children = {
    { kind = "row", align_items = "center", gap = 8, children = {
      { kind = "region", id = "back", w = 44, h = 44, draw = function() end },
      { kind = "label", id = "title", text = "Clock", font = fonts.large, bold = true, grow = 1 },
    } },
    { kind = "column", id = "pv", h = 196, border = true, radius = 16, children = {} },
    { kind = "radio", id = "face", items = ITEMS, value = value, columns = 1, gap = 8, h = 3 * 56 + 2 * 8 },
    { kind = "label", id = "hint", text = HINT, font = fonts.small, h = "fit" },
    { kind = "spacer" },
  } }
end

local function frame(g, value)
  local t = tree(value)
  t.x, t.y, t.w, t.h = 0, 0, 480, 800
  ui:begin_frame(g)
  ui:column("page", t)
  ui:end_frame()
end

local function box(t)
  return { t.x, t.y, t.w, t.h }
end

local function key(name, phase)
  return ui:handle_event({ type = "key", key = name, phase = phase or "down", time_ms = 0 })
end

local function show(...)
  local r = table.pack(...)
  for i = 1, r.n do r[i] = tostring(r[i]) end
  return table.concat(r, " ", 1, r.n)
end

function on_load(ctx)
  fonts = ctx.fonts
  ui = ctx.ui:create({ focus_style = "outline" })
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then
    frame(g, 2)
    return
  end
  done = true
  local s, m, l = fonts.small, fonts.medium, fonts.large
  local b = ui:layout({ x = 0, y = 0, w = 480, h = 800 }, tree(2))
  T.value("tree", B.text(b))
  -- everything above the hint is font-independent except the title's line height
  T.value("back", box(b.back))
  T.value("pv", box(b.pv))
  T.value("pv_has_no_children", #b.pv == 0)
  T.value("face", box(b.face))
  T.value("title", b.title.x == 68 and b.title.y == 16 + (44 - l.line_height) // 2 and b.title.w == 396
    and b.title.h == l.line_height)
  local _, hint_h = s:layout(HINT, 448, 32767)
  T.value("hint", b.hint.x == 16 and b.hint.y == 488 and b.hint.w == 448 and b.hint.h == hint_h)
  T.value("spacer_takes_the_rest", b[5].y == 488 + hint_h + 16 and b[5].h == 784 - (488 + hint_h + 16))

  -- entering the page the app focuses the radio: its value item, with the ring after that cell's text
  ui:focus("face")
  frame(g, 2)
  T.value("enter_focus", show(ui:focused()))
  T.value("down", show(key("down")))
  local log = L.start(g)
  frame(g, 2)
  local function cell(n, value)
    local y = 288 + (n - 1) * 64
    local out = { L.call("circle", 32, y + 28, 16, { line_width = 2 }) }
    if n == value then out[#out + 1] = L.call("circle", 32, y + 28, 11, { fill = true }) end
    out[#out + 1] = L.call("text_box", 56, y, 408, 56, ITEMS[n], { align = "left", color = 15, font = m,
      overflow = "ellipsis", valign = "middle", wrap = "none" })
    return out
  end
  local want = { L.call("rect", 16, 76, 448, 196, { color = 15, line_width = 1, radius = 16 }) }
  for n = 1, 3 do
    for _, c in ipairs(cell(n, 2)) do want[#want + 1] = c end
    if n == 3 then want[#want + 1] = L.call("rect", 13, 413, 454, 62, { line_width = 2, radius = 0 }) end
  end
  local got = {}
  for _, c in ipairs(log) do
    if c:sub(1, 7) == "circle(" or c:find("^rect%(") or (c:find("^text_box%(56,") ~= nil) then got[#got + 1] = c end
  end
  T.value("border_cells_and_ring", L.text(got) == L.text(want))
  T.value("ok_down", show(key("ok", "down")))
  T.value("ok_up", show(key("ok", "up")))
  frame(g, 3)
  local walk = {}
  for _, k in ipairs({ "down", "up", "up", "up" }) do walk[#walk + 1] = k .. ":" .. show(key(k)) end
  frame(g, 3)
  walk[#walk + 1] = "down:" .. show(key("down"))
  frame(g, 3)
  walk[#walk + 1] = "right:" .. show(key("right"))
  T.value("walk", walk)
  T.value("tap_item_2", show(ui:handle_event({ type = "pointer", phase = "up", x = 100, y = 362, down_x = 100,
    down_y = 362, gesture = "tap", time_ms = 0 })))
  -- the pixels are read from this frame alone, not from the ink of the frames above
  g:clear(0)
  frame(g, 2)
  T.value("focus_after_tap", show(ui:focused()))
end
