local T = require("conformance")

local function probe(f, s)
  local r, e = f(s)
  if r then return #r end
  return nil, e
end

function on_load(ctx)
  local big = string.rep("a", 1048577)
  local max = string.sub(big, 1, 1048576)
  T.call("hex_max", function() return probe(function(s) return ctx.hex:encode(s) end, max) end)
  T.call("hex_over", function() return probe(function(s) return ctx.hex:encode(s) end, big) end)
  T.call("base64_max", function() return probe(function(s) return ctx.base64:encode(s) end, max) end)
  T.call("base64_over", function() return probe(function(s) return ctx.base64:encode(s) end, big) end)
  T.call("url_over", function() return probe(function(s) return ctx.url:encode(s) end, big) end)
end
