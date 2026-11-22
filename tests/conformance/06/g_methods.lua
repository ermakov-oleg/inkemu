local T = require("conformance")

function on_draw(ctx, g)
  for _, name in ipairs({ "round_rect", "measure", "measure_text", "fill_pattern", "pattern_rect" }) do
    T.type("absent." .. name, g[name])
  end
end
