local T = require("conformance")

function on_load(ctx)
  local f = ctx.fonts.medium
  local lh = f.line_height
  -- every result is lines, height - expected height, consumed
  T.call("types", function()
    local l, h, c = f:layout("abc", 200, 200)
    return math.type(l), math.type(h), math.type(c)
  end)
  T.call("three_lines", function()
    local l, h, c = f:layout("a\nb\nc", 200, 500)
    return l, h - l * lh, c
  end)
  T.call("line_gap", function()
    local l, h, c = f:layout("a\nb\nc", 200, 500, { line_gap = 4 })
    return l, h - (l * lh + (l - 1) * 4), c
  end)
  T.call("clip_two_lines", function()
    local l, h, c = f:layout("a\nb\nc\nd", 200, 2 * lh)
    return l, h - l * lh, c
  end)
  T.call("nothing_fits", function() return f:layout("a\nb", 200, lh - 1) end)
  T.call("visible_ignores_height", function()
    local l, h, c = f:layout("a\nb\nc", 200, 1, { overflow = "visible" })
    return l, h - l * lh, c
  end)
end
