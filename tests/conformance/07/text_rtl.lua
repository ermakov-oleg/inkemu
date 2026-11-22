local T = require("conformance")

function on_draw(ctx, g)
  T.call("hebrew_text", function() return g:text(0, 0, "abc \u{05D0}\u{05D1}") end)
  T.call("arabic_text", function() return g:text(0, 0, "\u{0627}\u{0644}") end)
  T.call("hebrew_box", function() return g:text_box(0, 0, 100, 100, "x \u{05D0}") end)
  T.call("latin_ok", function() return g:text(0, 0, "abc") end)
end
