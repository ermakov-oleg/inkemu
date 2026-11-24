local T = require("conformance")

function on_load(ctx)
  local f = ctx.fonts.medium
  local text = string.rep("Hello w\xc3\xb6rld \xe6\xbc\xa2\xe5\xad\x97 test. ", 40)
  local page_h = 3 * f.line_height
  T.call("paging", function()
    local pos, pages, boundary, max_lines = 1, 0, true, 0
    while pos <= #text and pages < 500 do
      local rest = text:sub(pos)
      local lines, _, consumed = f:layout(rest, 200, page_h)
      if lines > max_lines then max_lines = lines end
      if consumed <= 0 then return false, false, false, false end
      local b = rest:byte(consumed + 1)
      if b and b >= 0x80 and b < 0xC0 then boundary = false end
      pos = pos + consumed
      pages = pages + 1
    end
    return boundary, pos == #text + 1, pages > 1, max_lines <= 3
  end)
end
