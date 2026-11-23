local T = require("conformance")

local function nearest(ctx, px)
  local best
  for _, name in ipairs({ "small", "medium", "large" }) do
    local f = ctx.fonts[name]
    if best == nil or math.abs(f.line_height - px) < math.abs(best.line_height - px) then best = f end
  end
  return best
end

function on_load(ctx)
  for _, px in ipairs({ 8, 16, 20, 22, 24, 27, 30, 40, 96 }) do
    T.call("tier_" .. px, function()
      local f = ctx.fonts:size(px)
      local want = nearest(ctx, px)
      return f.line_height == want.line_height and f.ascent == want.ascent and f.px == nil
    end)
  end
  T.call("size_24_is_medium", function() return ctx.fonts:size(24) == ctx.fonts.medium end)
  T.call("same_object", function() return ctx.fonts:size(40) == ctx.fonts:size(40) end)
end
