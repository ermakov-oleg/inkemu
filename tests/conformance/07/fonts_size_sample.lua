local T = require("conformance")

local function got(...)
  local f, e = ctx.fonts:size(...)
  return f ~= nil, e
end

function on_load(ctx)
  T.call("sample_ascii", function() return got(24, "abc") end)
  T.call("sample_empty", function() return got(24, "") end)
  T.call("sample_nil", function() return got(24, nil) end)
  T.call("sample_bad_utf8", function() return ctx.fonts:size(24, "a\xffb") end)
  T.call("sample_nul", function() return ctx.fonts:size(24, "a\0b") end)
  T.call("sample_rtl", function() return ctx.fonts:size(24, "\u{05D0}") end)
  T.value("raises_table", not pcall(function() return ctx.fonts:size(24, {}) end))
  T.value("raises_boolean", not pcall(function() return ctx.fonts:size(24, true) end))
end
