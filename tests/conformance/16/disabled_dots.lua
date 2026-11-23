local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

function on_draw(ctx, g)
  g:clear(0)
  local seen = {}
  ui:begin_frame(g)
  ui:button("off", { x = 20, y = 20, w = 200, h = 56, text = "", enabled = false, selected = true, badge = "9" })
  ui:button("on", { x = 20, y = 100, w = 200, h = 56, text = "" })
  ui:region("region_off", { x = 20, y = 180, w = 200, h = 56, enabled = false, draw = function(_, _, st)
    seen.enabled = st.enabled
    L.mark("draw")
  end })
  local log = L.start(g)
  ui:end_frame()
  local dots = L.call("pattern", 20, 20, 200, 56, { color = 0, density = 50, kind = "dot" })
  local patterns = 0
  for _, c in ipairs(log) do
    if string.sub(c, 1, 8) == "pattern(" then patterns = patterns + 1 end
  end
  -- the disabled button: fill, text, the badge's three calls, then the dots over all of it
  T.value("button.dots_last", log[6] == dots)
  T.value("patterns", patterns)
  T.value("region.no_dots", log[#log] == "draw")
  T.value("region.st_enabled", seen.enabled)
  T.value("calls", L.text(log))
end
