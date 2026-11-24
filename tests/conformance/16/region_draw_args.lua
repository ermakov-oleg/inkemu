local T = require("conformance")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

function on_draw(ctx, g)
  g:clear(0)
  local calls = {}
  local function draw(id)
    return function(g2, r, st)
      calls[#calls + 1] = { id = id, g = rawequal(g2, g), r = r, st = st }
    end
  end
  local box = { x = 10, y = 20, w = 30, h = 40, draw = draw("a") }
  ui:begin_frame(g)
  ui:region("a", box)
  T.value("called_when_declared", #calls)
  ui:region("b", { x = 50, y = 20, w = 30, h = 40, selected = true, enabled = false, draw = draw("b") })
  T.call("no_draw", function() return ui:region("c", { x = 90, y = 20, w = 30, h = 40 }) end)
  ui:end_frame()
  T.value("order", #calls == 2 and calls[1].id .. calls[2].id)
  T.value("a.g_is_frame_canvas", calls[1].g)
  T.value("a.r", calls[1].r)
  T.value("a.r_is_new_table", not rawequal(calls[1].r, box))
  T.value("a.st", calls[1].st)
  T.value("b.st", calls[2].st)
  -- an error raised by draw comes out of end_frame, and the frame is closed all the same
  ui:begin_frame(g)
  ui:region("boom", { x = 0, y = 0, w = 10, h = 10, draw = function() error("draw failed", 0) end })
  T.call("end_with_failing_draw", function() return ui:end_frame() end)
  T.call("begin_after_failing_draw", function() return ui:begin_frame(g) end)
  ui:end_frame()
end
