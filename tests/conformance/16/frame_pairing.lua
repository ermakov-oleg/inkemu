local T = require("conformance")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

function on_draw(ctx, g)
  g:clear(0)
  -- the script's own drawing; an empty frame must leave it on screen
  g:rect(10, 10, 40, 40, { fill = true })
  T.call("end_without_begin", function() return ui:end_frame() end)
  T.call("begin_nil", function() return ui:begin_frame(nil) end)
  T.call("begin_plain_table", function() return ui:begin_frame({}) end)
  T.call("end_after_refused_begin", function() return ui:end_frame() end)
  T.call("begin", function() return ui:begin_frame(g) end)
  T.call("begin_again", function() return ui:begin_frame(g) end)
  T.call("end", function() return ui:end_frame() end)
  T.call("end_twice", function() return ui:end_frame() end)
  ui:begin_frame(g)
  ui:end_frame()
end
