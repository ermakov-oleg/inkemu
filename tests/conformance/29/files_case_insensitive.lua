local T = require("conformance")

local function items(list)
  local out = {}
  for i, e in ipairs(list) do out[i] = { name = e.name, size = e.size } end
  return out
end

function on_load(ctx)
  local F = ctx.files
  F:write("Book.TXT", "1")
  T.call("list_spelling", function() return items(F:list()) end)
  T.call("read_other_case", function() return F:read("book.txt") end)
  T.call("exists_other_case", function() return F:exists("BOOK.txt") end)
  T.call("size_other_case", function() return F:size("bOOK.TXT") end)
  F:write("book.txt", "22")
  T.call("read_first_case", function() return F:read("Book.TXT") end)
  T.value("count_after", #F:list())
  T.call("remove_other_case", function() return F:remove("BOOK.TXT") end)
  T.value("count_removed", #F:list())
end
