local T = require("conformance")

function on_draw(ctx, g)
  T.call("latin_ok", function() return g:text(0, 0, "abc") end)
end
