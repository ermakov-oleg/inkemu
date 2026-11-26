local T = require("conformance")
local ui
local rows

function on_load(ctx)
  ui = ctx.ui:create()
end

-- what the row callback saw, as "index@y/focused/selected" per row
local function draw(_, r, i, st)
  rows[#rows + 1] = i .. "@" .. r.y .. "/" .. tostring(st.focused) .. "/" .. tostring(st.selected)
end

local function frame(g, o)
  rows = {}
  ui:begin_frame(g)
  ui:button("top", { x = 10, y = 20, w = 200, h = 56, text = "top" })
  o.x, o.y, o.w, o.h, o.draw = 10, 100, 460, o.h or 448, draw
  local _, err = ui:list("goods", o)
  ui:button("bottom", { x = 10, y = 600, w = 200, h = 56, text = "bottom" })
  ui:end_frame()
  return err or "ok"
end

local function send(name, ev)
  T.call(name, function() return ui:handle_event(ev) end)
end

local function key(name, phase)
  return { type = "key", key = name, phase = phase or "down", time_ms = 0 }
end

local function list(g, page, selected)
  frame(g, { count = 20, row_h = 64, page = page, selected = selected })
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then return end
  done = true
  -- 448 // 64 = 7 rows a page, 3 pages; a page past the last is the last
  frame(g, { count = 20, row_h = 64, page = 1, selected = 3 })
  T.value("page_1", table.concat(rows, " "))
  frame(g, { count = 20, row_h = 64, page = 9 })
  T.value("page_past_last", table.concat(rows, " "))
  frame(g, { count = 9, row_h = 100, page = 2, h = 450 })
  T.value("leftover_space", table.concat(rows, " "))
  frame(g, { count = 0, row_h = 64 })
  T.value("empty", #rows)
  T.value("rows_cost", { frame(g, { count = 200, row_h = 4, h = 400 }), #rows })
  T.value("rows_limit", frame(g, { count = 200, row_h = 3, h = 400 }))
  T.value("bad_row_h", frame(g, { count = 20, row_h = 449 }))
  T.value("bad_selected", frame(g, { count = 20, row_h = 64, selected = 0 }))
  -- keys walk the rows; past the page's last row the list asks for the next page, focus on the next row
  list(g, 1, 3)
  T.call("focus_row", function() return ui:focus("goods", 7) end)
  list(g, 1, 3)
  send("down_turns", key("down"))
  list(g, 2, 3)
  T.value("page_2_focus", table.concat(rows, " "))
  send("up_turns_back", key("up"))
  list(g, 1, 3)
  send("up_walks", key("up"))
  -- the kept row is not on the page: the selected row of the page, else its first row
  T.call("focus_off_page", function() return ui:focus("goods", 3) end)
  list(g, 2, 10)
  T.call("focused_selected", function() return ui:focused() end)
  T.call("focus_off_page_2", function() return ui:focus("goods", 3) end)
  list(g, 2, 30)
  T.call("focused_first", function() return ui:focused() end)
  send("ok_down", key("ok"))
  send("ok_up", key("ok", "up"))
  -- on the last page the last row hands the key on
  list(g, 3)
  T.call("focus_last", function() return ui:focus("goods", 20) end)
  list(g, 3)
  send("down_leaves", key("down"))
end
