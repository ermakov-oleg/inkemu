local T = require("conformance")

local function measured(f, s)
  local w, e = f:measure(s)
  return type(w), e
end

function on_draw(ctx, g)
  local f = ctx.fonts.small
  local ok, over = string.rep("a", 8192), string.rep("a", 8193)
  T.call("text_8192", function() return g:text(0, 0, ok) end)
  T.call("text_8193", function() return g:text(0, 0, over) end)
  T.call("box_8192", function() return g:text_box(0, 0, 100, 100, ok) end)
  T.call("box_8193", function() return g:text_box(0, 0, 100, 100, over) end)
  T.call("measure_8192", function() return measured(f, ok) end)
  T.call("measure_8193", function() return measured(f, over) end)
end
