local T = require("conformance")

local function got(ctx, ...)
  local f, e = ctx.fonts:size(...)
  return f ~= nil, e
end

function on_load(ctx)
  T.call("sample_ascii", function() return got(ctx, 24, "abc") end)
  T.call("sample_empty", function() return got(ctx, 24, "") end)
  T.call("sample_nil", function() return got(ctx, 24, nil) end)
end
