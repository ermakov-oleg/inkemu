local T = require("conformance")

function on_draw(ctx, g)
  for _, name in ipairs({ "size", "clear", "line", "rect", "circle", "arc", "polyline", "polygon", "pattern", "text", "text_box", "image" }) do
    T.type("has." .. name, g[name])
  end
  for _, name in ipairs({ "round_rect", "measure", "measure_text", "fill_pattern", "pattern_rect" }) do
    T.type("absent." .. name, g[name])
  end
end
