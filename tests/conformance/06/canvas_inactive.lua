local T = require("conformance")
local saved, scheduled = nil, false

function on_draw(ctx, g)
  saved = g
  if scheduled then return end
  scheduled = true
  ctx:after(100, function()
    T.call("rect_after_draw", function() return saved:rect(0, 0, 5, 5, { fill = true }) end)
    T.call("size_after_draw", function() return saved:size() end)
  end)
end
