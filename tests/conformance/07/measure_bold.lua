local T = require("conformance")

function on_load(ctx)
  local text = "Hello, wide world"
  for _, name in ipairs({ "small", "medium", "large" }) do
    local f = ctx.fonts[name]
    T.call("bold_delta_" .. name, function() return f:measure(text, { bold = true }) - f:measure(text) end)
  end
  local f = ctx.fonts.medium
  T.call("bold_empty", function() return f:measure("", { bold = true }) end)
  T.call("bold_false", function() return f:measure(text, { bold = false }) - f:measure(text) end)
  T.call("plain_opts", function() return f:measure(text, {}) - f:measure(text) end)
  T.call("italic_ignored", function()
    return f:measure(text, { italic = true, underline = true, strike = true }) - f:measure(text)
  end)
end
