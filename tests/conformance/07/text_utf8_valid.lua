local T = require("conformance")

function on_draw(ctx, g)
  T.call("text_ok", function() return g:text(0, 0, "caf\xc3\xa9") end)
end
