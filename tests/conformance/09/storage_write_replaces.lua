local T = require("conformance")

function on_load(ctx)
  local S = ctx.storage
  T.call("write1", function() return S:write("a", "abc") end)
  T.call("write2", function() return S:write("a", "de") end)
  T.call("read", function() return S:read("a") end)
  T.call("size", function() return S:size("a") end)
  T.call("write_binary", function() return S:write("bin", "\0\255\n\0") end)
  T.call("read_binary", function() return T.hex(S:read("bin")) end)
  T.call("write_empty", function() return S:write("empty", "") end)
  T.call("read_empty", function() return S:read("empty") end)
  T.call("size_empty", function() return S:size("empty") end)
end
