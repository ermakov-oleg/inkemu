local T = require("conformance")
local B = require("layout_box")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
  local b = ui:layout({ x = 0, y = 0, w = 100, h = 100 }, { kind = "column", gap = 5, children = {
    { id = "a", h = 10 }, false, { id = "gone", h = 10, hidden = true }, { id = "b", h = 10 } } })
  T.value("skipped", B.text(b))
  T.value("ids", { b.a == b[1], b.b == b[4], b.gone == nil })
  -- a hidden child's fields are still checked
  T.call("hidden_bad_field", function()
    return ui:layout({ x = 0, y = 0, w = 100, h = 100 }, { kind = "column", children = { { h = 10, hidden = true, grow = -1 } } })
  end)
  T.call("hidden_not_boolean", function()
    return ui:layout({ x = 0, y = 0, w = 100, h = 100 }, { kind = "column", children = { { h = 10, hidden = 1 } } })
  end)
  local z = ui:layout({ x = 0, y = 0, w = 100, h = 20 }, { kind = "column", children = {
    { id = "a", h = 20, shrink = 0 }, { id = "z", h = 10, min_h = 0 } } })
  T.value("zero_height", B.text(z))
end

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  T.call("frame", function()
    return ui:column("col", { x = 0, y = 0, w = 200, h = 100, children = {
      false,
      { kind = "button", id = "hid", text = "H", h = 40, hidden = true },
      { kind = "button", id = "zero", text = "Z", h = 40, min_h = 0 },
      { kind = "button", id = "keep", text = "K", h = 100, shrink = 0 },
    } })
  end)
  -- a hidden or zero-size control takes no id: a later control may use it
  T.call("ids_free", function() return ui:button("hid", { x = 300, y = 0, w = 10, h = 10 }) end)
  ui:end_frame()
  T.value("focus", ui:focused())
  T.value("tap_zero", { ui:handle_event({ type = "pointer", phase = "up", gesture = "tap", x = 100, y = 99 }) })
end
