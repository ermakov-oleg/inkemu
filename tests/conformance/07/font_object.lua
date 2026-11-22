local T = require("conformance")

function on_load(ctx)
  for _, name in ipairs({ "small", "medium", "large" }) do
    local f = ctx.fonts[name]
    T.value(name .. ".name", f.name)
    T.type(name .. ".line_height", f.line_height)
    T.type(name .. ".ascent", f.ascent)
    T.type(name .. ".measure", f.measure)
    T.type(name .. ".layout", f.layout)
    T.type(name .. ".px", f.px)
  end
  local s = ctx.fonts:size(24)
  T.type("sized.line_height", s.line_height)
  T.type("sized.ascent", s.ascent)
  T.type("sized.measure", s.measure)
  T.type("sized.layout", s.layout)
end
