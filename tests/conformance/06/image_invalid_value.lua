local T = require("conformance")

function on_draw(ctx, g)
  T.call("number", function() return g:image(42, 0, 0) end)
  T.call("string", function() return g:image("icon", 0, 0) end)
  T.call("plain_table", function() return g:image({}, 0, 0) end)
end
