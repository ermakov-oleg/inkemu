local T = require("conformance")

function on_load(ctx)
  for _, name in ipairs({ "small", "medium", "large" }) do
    T.value(name .. ".ascent", ctx.fonts[name].ascent)
  end
end
