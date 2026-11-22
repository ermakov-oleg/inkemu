local T = require("conformance")

local function measured(f, s)
  local w, e = f:measure(s)
  return type(w), e
end

function on_draw(ctx, g)
  local f = ctx.fonts.small
  local over = string.rep("a", 8193)
  -- breakable text for text_box: one long unbreakable word is wrapped in quadratic time by the 0.1 wrapper
  local box_over = string.rep("a ", 4096) .. "a"
  T.call("text_8193", function() return g:text(0, 0, over) end)
  T.call("box_8193", function() return g:text_box(0, 0, 100, 100, box_over) end)
  T.call("measure_8193", function() return measured(f, over) end)
end
