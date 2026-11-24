local T = require("conformance")
local B = require("layout_box")
local L = require("canvas_log")
local ui, fonts

-- A help page in the shape of the "Today" app's setup screen: a back region and a title, a long small label that
-- wraps (h = "fit"), one the app broke into lines itself, a large tabular code and a spacer.
local BODY = "Copy the package to the card, open it from the file list and confirm; the old build stays next to it."
local LINES = "Menu, Files, Apps\nthen the package"
local CODE = "4711-0815"

local function tree()
  return { kind = "column", padding = 16, gap = 16, children = {
    { kind = "row", align_items = "center", gap = 8, children = {
      { kind = "region", id = "back", w = 44, h = 44, draw = function() end },
      { kind = "label", id = "title", text = "Install", font = fonts.large, bold = true, grow = 1 },
    } },
    { kind = "label", id = "body", text = BODY, font = fonts.small, h = "fit" },
    { kind = "label", id = "lines", text = LINES, font = fonts.small, h = "fit" },
    { kind = "label", id = "code", text = CODE, font = fonts.large, tabular = true, h = "fit", align = "center" },
    { kind = "spacer" },
  } }
end

local function frame(g)
  local t = tree()
  t.x, t.y, t.w, t.h = 0, 0, 480, 800
  ui:begin_frame(g)
  ui:column("page", t)
  ui:end_frame()
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
  local b = ui:layout({ x = 0, y = 0, w = 480, h = 800 }, tree())
  T.value("tree", B.text(b))
  local _, body_h = s:layout(BODY, 448, 32767)
  local lines_y = 76 + body_h + 16
  local code_y = lines_y + 2 * s.line_height + 16
  local rest = code_y + l.line_height + 16
  T.value("body_wraps", body_h > s.line_height)
  T.value("body", b.body.x == 16 and b.body.y == 76 and b.body.w == 448 and b.body.h == body_h)
  T.value("lines", b.lines.y == lines_y and b.lines.h == 2 * s.line_height)
  T.value("code", b.code.x == 16 and b.code.y == code_y and b.code.w == 448 and b.code.h == l.line_height)
  T.value("spacer_takes_the_rest", b[5].y == rest and b[5].h == 784 - rest)

  local log = L.start(g)
  frame(g)
  local text = L.text(log)
  local function has(...)
    return string.find(text, L.call("text_box", ...), 1, true) ~= nil
  end
  T.value("body_box", has(16, 76, 448, body_h, BODY, { font = s, overflow = "ellipsis" }))
  T.value("code_box", has(16, code_y, 448, l.line_height, CODE, { align = "center", font = l, overflow = "ellipsis",
    tabular = true }))
  T.value("first_focus", show(ui:focused()))
  T.value("down", show(ui:handle_event({ type = "key", key = "down", phase = "down", time_ms = 0 })))
  T.value("up", show(ui:handle_event({ type = "key", key = "up", phase = "down", time_ms = 0 })))
  frame(g)
  T.value("focus_kept", show(ui:focused()))
end
