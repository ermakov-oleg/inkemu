local T = require("conformance")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

local function button(name, id, o)
  T.call(name, function() return ui:button(id, o) end)
end

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  button("valid", "ok1", { x = 20, y = 20, w = 100, h = 40, text = "a" })
  button("empty_id", "", { x = 20, y = 80, w = 100, h = 40, text = "b" })
  button("number_id", 5, { x = 20, y = 140, w = 100, h = 40, text = "c" })
  button("repeated_id", "ok1", { x = 20, y = 200, w = 100, h = 40, text = "d" })
  T.call("repeated_id_label", function() return ui:label("ok1", { x = 20, y = 260, w = 100, h = 40, text = "e" }) end)
  T.call("no_opts", function() return ui:button("bare") end)
  button("no_x", "nox", { y = 320, w = 100, h = 40 })
  button("no_w", "now", { x = 20, y = 320, h = 40 })
  button("no_h", "noh", { x = 20, y = 320, w = 100 })
  button("w_zero", "w0", { x = 20, y = 320, w = 0, h = 40 })
  button("h_zero", "h0", { x = 20, y = 320, w = 100, h = 0 })
  button("w_fraction", "wf", { x = 20, y = 320, w = 10.5, h = 40 })
  button("x_fraction", "xf", { x = 20.5, y = 320, w = 100, h = 40 })
  button("w_fit", "wfit", { x = 20, y = 320, w = "fit", h = 40 })
  button("w_percent", "wpct", { x = 20, y = 320, w = "50%", h = 40 })
  button("enabled_not_boolean", "en", { x = 20, y = 320, w = 100, h = 40, enabled = "no" })
  button("border_not_boolean", "bo", { x = 20, y = 320, w = 100, h = 40, border = 1 })
  T.call("draw_not_function", function() return ui:region("rg", { x = 20, y = 320, w = 100, h = 40, draw = 5 }) end)
  button("x_whole_float", "x2", { x = 2.0, y = 400, w = 10, h = 40 })
  button("negative_xy", "neg", { x = -30, y = -10, w = 50, h = 40 })
  ui:end_frame()
end
