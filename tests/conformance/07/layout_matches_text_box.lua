local T = require("conformance")

local function nret(...) return select("#", ...) end

function on_draw(ctx, g)
  local f = ctx.fonts.medium
  local text = "alpha beta gamma delta epsilon zeta eta theta"
  T.call("fit", function()
    local n = f:layout(text, 100, 800)
    for _ = 1, 1024 - n do g:rect(0, 0, 1, 1, { fill = true, color = 0 }) end
    local box = nret(g:text_box(0, 100, 100, 800, text))
    local extra_ok, extra_err = g:rect(0, 0, 1, 1, { fill = true, color = 0 })
    return n > 1, box, extra_ok, extra_err
  end)
end
