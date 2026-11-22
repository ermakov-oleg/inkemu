local T = require("conformance")

function on_draw(ctx, g)
  local f = ctx.fonts.medium
  T.call("text", function() return g:text(0, 0, "bad\xffbyte") end)
  T.call("text_cut", function() return g:text(0, 0, "cut\xc3") end)
  T.call("text_box", function() return g:text_box(0, 0, 100, 100, "bad\xffbyte") end)
  T.call("measure", function() return f:measure("bad\xffbyte") end)
  T.call("layout", function() return f:layout("bad\xffbyte", 100, 100) end)
end
