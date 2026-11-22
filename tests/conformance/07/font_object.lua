local T = require("conformance")

function on_load(ctx)
  for _, name in ipairs({ "small", "medium", "large" }) do
    T.type(name .. ".layout", ctx.fonts[name].layout)
  end
  T.type("sized.layout", ctx.fonts:size(24).layout)
end
