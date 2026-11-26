local T = require("conformance")

-- each call gets one option of the right type with a value its rule refuses
local cases = {
  rect_empty = { rect = { x = 0, y = 0, w = 0, h = 10 } },
  rect_off_screen = { rect = { x = 0, y = 0, w = 481, h = 10 } },
  rect_fraction = { rect = { x = 0.5, y = 0, w = 10, h = 10 } },
  text_newline = { text = "a\nb" },
  text_too_long = { text = "abcd", max_chars = 3 },
  text_utf8 = { text = "\255" },
  max_chars_low = { max_chars = 0 },
  max_chars_high = { max_chars = 513 },
  max_chars_fraction = { max_chars = 2.5 },
  placeholder_long = { placeholder = string.rep("p", 129) },
  kind = { kind = "email" },
  enter = { enter = "go" },
  newline_single_line = { enter = "newline" },
}

function on_load(ctx)
  for name, opts in pairs(cases) do
    T.call(name, function() return ctx.input:edit(opts) end)
  end
end
