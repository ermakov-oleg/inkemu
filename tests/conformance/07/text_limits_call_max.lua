local T = require("conformance")

local function measured(f, s)
  local w, e = f:measure(s)
  return type(w), e
end

function on_draw(ctx, g)
  local f = ctx.fonts.small
  local ok = string.rep("a", 8192)
  -- breakable text for text_box: one long unbreakable word is wrapped in quadratic time by the 0.1 wrapper
  local box_ok = string.rep("a ", 4096)
  T.call("text_8192", function() return g:text(0, 0, ok) end)
  T.call("box_8192", function() return g:text_box(0, 0, 100, 100, box_ok) end)
  T.call("measure_8192", function() return measured(f, ok) end)
end
