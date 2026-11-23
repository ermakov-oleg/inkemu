local T = require("conformance")
local ui

-- one frame of buttons `{id, x[, enabled]}` in a row (all 100 x 60 at y = 300), then the focus it settled on
local function frame(g, buttons)
  ui:begin_frame(g)
  for _, b in ipairs(buttons) do
    ui:button(b[1], { x = b[2], y = 300, w = 100, h = 60, text = b[1], enabled = b[3] })
  end
  ui:end_frame()
  return ui:focused()
end

function on_load(ctx)
  ui = ctx.ui:create()
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then
    frame(g, { { "A", 40 }, { "B", 190 }, { "C", 340 } })
    return
  end
  done = true
  T.value("new_object", ui:focused())
  T.value("first_frame", frame(g, { { "A", 40 }, { "B", 190 }, { "C", 340 } }))
  ui:focus("B")
  T.value("kept", frame(g, { { "A", 40 }, { "B", 190 }, { "C", 340 } }))
  T.value("absent", frame(g, { { "A", 40 }, { "C", 340 } }))
  ui:focus("B")
  T.value("disabled_tie", frame(g, { { "A", 40 }, { "B", 190, false }, { "C", 340 } }))
  ui:focus("B")
  T.value("disabled_nearest", frame(g, { { "A", 40 }, { "B", 190, false }, { "C", 300 } }))
  T.value("absent_skips_disabled", frame(g, { { "X", 40, false }, { "C", 340 } }))
  T.value("all_disabled", frame(g, { { "A", 40, false }, { "B", 190, false } }))
  T.value("after_empty", frame(g, { { "A", 40, false }, { "C", 340 } }))
  ui:begin_frame(g)
  ui:label("only", { x = 0, y = 0, w = 100, h = 30, text = "L" })
  ui:end_frame()
  T.value("no_hittable", ui:focused())
  ui:focus("lbl")
  ui:begin_frame(g)
  ui:button("A", { x = 40, y = 300, w = 100, h = 60, text = "A" })
  ui:button("B", { x = 190, y = 300, w = 100, h = 60, text = "B" })
  ui:label("lbl", { x = 0, y = 0, w = 100, h = 30, text = "L" })
  ui:end_frame()
  T.value("label_id", ui:focused())
  -- two objects do not share focus, and an object with no frame drawn leaves its focus alone
  local other = ctx.ui:create()
  ui:focus("B")
  frame(g, { { "A", 40 }, { "B", 190 } })
  other:begin_frame(g)
  other:button("A", { x = 40, y = 400, w = 100, h = 60, text = "A" })
  other:end_frame()
  T.value("objects", { ui:focused(), other:focused() })
end
