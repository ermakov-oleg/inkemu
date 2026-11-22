local T = require("conformance")

function on_load(ctx)
  local text = "Hello, wide world"
  for _, name in ipairs({ "small", "medium", "large" }) do
    local f = ctx.fonts[name]
    T.call("bold_delta_" .. name, function() return f:measure(text, { bold = true }) - f:measure(text) end)
  end
end
