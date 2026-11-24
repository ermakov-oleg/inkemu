local T = require("conformance")

local function hex_of(r, e)
  if r then return T.hex(r) end
  return nil, e
end

function on_load(ctx)
  local B = ctx.base64
  T.call("url", function() return hex_of(B:decode("-_-_")) end)
  T.call("unpadded", function() return B:decode("YWI") end)
end
