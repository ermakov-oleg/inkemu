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
end
