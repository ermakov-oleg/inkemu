local T = require("conformance")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

local function buttons(first, n, ok)
  for i = first, first + n - 1 do
    local r = table.pack(ui:button("b" .. i, { x = (i - 1) % 8 * 60, y = (i - 1) // 8 * 40, w = 50, h = 30 }))
    if r.n == 0 then ok = ok + 1 end
  end
  return ok
end

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  T.value("buttons_128.accepted", buttons(1, 128, 0))
  T.call("buttons_129th", function() return ui:button("x", { x = 300, y = 700, w = 100, h = 50 }) end)
  ui:end_frame()
  -- labels count too
  ui:begin_frame(g)
  local ok = 0
  for i = 1, 60 do
    local r = table.pack(ui:label("l" .. i, { x = 0, y = 500, w = 10, h = 10, text = "" }))
    if r.n == 0 then ok = ok + 1 end
  end
  T.value("labels_and_buttons.accepted", buttons(1, 68, ok))
  T.call("labels_and_buttons_129th", function() return ui:button("x", { x = 300, y = 700, w = 100, h = 50 }) end)
  ui:end_frame()
  -- a radio is one control plus one per item
  local function radio_after_120(n)
    ui:begin_frame(g)
    buttons(1, 120, 0)
    local items = {}
    for i = 1, n do items[i] = "i" .. i end
    local r = table.pack(ui:radio("r", { x = 0, y = 400, w = 300, h = 200, items = items }))
    ui:end_frame()
    return r
  end
  T.value("radio_7_items_fills_128", radio_after_120(7).n)
  T.call("radio_8_items_is_129", function()
    local r = radio_after_120(8)
    return table.unpack(r, 1, r.n)
  end)
end
