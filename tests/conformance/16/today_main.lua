local T = require("conformance")
local B = require("layout_box")
local L = require("canvas_log")
local ui, fonts

-- A settings page in the shape of the "Today" app's main screen: a title row with a 44 px refresh region, two
-- groups (a small caption, then 56 px region rows with 1 px dividers), a disabled info row, an emphasis button, a
-- wrapped note and a spacer.
local function row(id, enabled)
  return { kind = "region", id = id, h = 56, enabled = enabled, draw = function(g, r, st)
    L.mark(string.format("draw:%s:%d,%d,%d,%d:%s", id, r.x, r.y, r.w, r.h, tostring(st.focused)))
  end }
end

local NOTE = "Rows open their own page; the reader asks before it changes the lock screen."

local function tree()
  return { kind = "column", padding = 16, gap = 12, children = {
    { kind = "row", align_items = "center", gap = 8, children = {
      { kind = "label", id = "title", text = "Agenda", font = fonts.large, bold = true, grow = 1 },
      row("refresh"),
    } },
    { kind = "label", id = "cap1", text = "SCREEN", font = fonts.small, h = "fit" },
    { kind = "column", divider = { line_width = 1 }, children = { row("style"), row("preview"), row("state", false) } },
    { kind = "button", id = "make", text = "Use as lock screen", h = 56, emphasis = true },
    { kind = "label", id = "cap2", text = "SOURCES", font = fonts.small, h = "fit" },
    { kind = "column", divider = { line_width = 1 }, children = { row("mail"), row("tasks") } },
    { kind = "label", id = "note", text = NOTE, font = fonts.small, h = "fit" },
    { kind = "spacer" },
  } }
end

local function frame(g)
  local t = tree()
  t.x, t.y, t.w, t.h = 0, 0, 480, 800
  t.children[1].children[2].w, t.children[1].children[2].h = 44, 44
  ui:begin_frame(g)
  ui:column("page", t)
  ui:end_frame()
end

local function spec()
  local t = tree()
  t.children[1].children[2].w, t.children[1].children[2].h = 44, 44
  return t
end

local function box(t)
  return { t.x, t.y, t.w, t.h }
end

local function same(t, x, y, w, h)
  return t.x == x and t.y == y and t.w == w and t.h == h
end

local function key(name, phase)
  return ui:handle_event({ type = "key", key = name, phase = phase or "down", time_ms = 0 })
end

local function tap(x, y)
  return ui:handle_event({ type = "pointer", phase = "up", x = x, y = y, down_x = x, down_y = y, gesture = "tap",
    time_ms = 0 })
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
    frame(g)
    return
  end
  done = true
  local s, l = fonts.small, fonts.large
  local b = ui:layout({ x = 0, y = 0, w = 480, h = 800 }, spec())
  T.value("tree", B.text(b))
  -- font-independent boxes
  T.value("head", box(b[1]))
  T.value("refresh", box(b.refresh))
  -- boxes that follow from the line heights and the wrapped note
  local _, note_h = s:layout(NOTE, 448, 32767)
  local cap1 = 16 + 44 + 12
  local rows1 = cap1 + s.line_height + 12
  local make = rows1 + 3 * 56 + 12
  local cap2 = make + 56 + 12
  local rows2 = cap2 + s.line_height + 12
  local note = rows2 + 2 * 56 + 12
  local rest = note + note_h + 12
  T.value("note_wraps", note_h > s.line_height)
  T.value("title_centred", same(b.title, 16, 16 + (44 - l.line_height) // 2, 448 - 8 - 44, l.line_height))
  T.value("captions", same(b.cap1, 16, cap1, 448, s.line_height) and same(b.cap2, 16, cap2, 448, s.line_height))
  T.value("rows", same(b[3], 16, rows1, 448, 168) and same(b.style, 16, rows1, 448, 56)
    and same(b.preview, 16, rows1 + 56, 448, 56) and same(b.state, 16, rows1 + 112, 448, 56)
    and same(b[6], 16, rows2, 448, 112) and same(b.tasks, 16, rows2 + 56, 448, 56))
  T.value("button", same(b.make, 16, make, 448, 56))
  T.value("note", same(b.note, 16, note, 448, note_h))
  T.value("spacer_takes_the_rest", same(b[8], 16, rest, 448, 784 - rest))
  T.value("no_overflow", b.overflow == nil)

  -- the frame draws the same boxes; dividers sit 1 px inside the row above, the disabled row gets st.enabled only
  local log = L.start(g)
  frame(g)
  local first = L.text(log)
  T.value("first_focus", show(ui:focused()))
  local function has(op, ...)
    return string.find(first, L.call(op, ...), 1, true) ~= nil
  end
  local line = { color = 15, fill = true }
  T.value("dividers", has("rect", 16, rows1 + 55, 448, 1, line) and has("rect", 16, rows1 + 111, 448, 1, line)
    and has("rect", 16, rows2 + 55, 448, 1, line))
  T.value("divider_count", select(2, string.gsub(first, "rect%(16,%d+,448,1,", "")))
  T.value("rows_drawn", string.find(first, string.format("draw:style:16,%d,448,56:false", rows1), 1, true) ~= nil
    and string.find(first, string.format("draw:state:16,%d,448,56:false", rows1 + 112), 1, true) ~= nil)
  T.value("emphasis_filled", has("rect", 16, make, 448, 56, { fill = true, radius = 0 }))

  -- the key walk of the oracle's main-screen scenario, a frame after every key
  local walk = {}
  for _, k in ipairs({ "down", "down", "down", "down", "down", "down", "down", "up", "up", "left", "right", "up", "up",
    "up" }) do
    walk[#walk + 1] = k .. ":" .. show(key(k))
    frame(g)
  end
  T.value("walk", walk)
  -- with the button focused, the outline focus frame comes right after its fill
  ui:focus("make")
  log = L.start(g)
  frame(g)
  local fill = L.call("rect", 16, make, 448, 56, { fill = true, radius = 0 })
  local ring = L.call("rect", 13, make - 3, 454, 62, { line_width = 2, radius = 0 })
  local at
  for i, c in ipairs(log) do
    if c == ring then at = i end
  end
  T.value("button_ring_after_fill", at ~= nil and log[at - 1] == fill)
  ui:focus("refresh")
  frame(g)
  T.value("ok_down", show(key("ok", "down")))
  T.value("ok_up", show(key("ok", "up")))
  T.value("tap_row", show(tap(240, rows1 + 66)))
  T.value("tap_disabled_row", show(tap(240, rows1 + 122)))
  T.value("focus_after_tap", show(ui:focused()))
  ui:focus("mail")
  frame(g)
  T.value("back_key", show(key("back", "up")))
  T.value("focus_after_back", show(ui:focused()))
end
