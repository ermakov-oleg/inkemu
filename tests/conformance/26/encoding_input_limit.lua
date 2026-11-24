local T = require("conformance")

local function probe(f, s)
  local r, e = f(s)
  if r then return #r end
  return nil, e
end

function on_load(ctx)
  local text = string.rep("a", 1048577)
  T.call("hex", function() return probe(function(s) return ctx.hex:encode(s) end, text) end)
  T.call("base64", function() return probe(function(s) return ctx.base64:encode(s) end, text) end)
  T.call("url", function() return probe(function(s) return ctx.url:encode(s) end, text) end)
end
