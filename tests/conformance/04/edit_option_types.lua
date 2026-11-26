local T = require("conformance")

-- each call gets one option of the wrong type; none of them may open an edit
local cases = {
  rect = { rect = 5 },
  rect_field = { rect = { x = 0, y = 0, w = 10 } },
  text = { text = 5 },
  placeholder = { placeholder = {} },
  max_chars = { max_chars = "5" },
  kind = { kind = 5 },
  multiline = { multiline = "yes" },
  enter = { enter = true },
  on_change = { on_change = 5 },
  on_end = { on_end = "x" },
}

function on_load(ctx)
  for name, opts in pairs(cases) do
    T.value(name .. "_raises", not pcall(ctx.input.edit, ctx.input, opts))
  end
  T.call("still_free", function()
    local ed = ctx.input:edit({})
    return ed ~= nil, ed and ed:close()
  end)
end
