local T = require("conformance")

function on_load(ctx)
  local N = ctx.net
  local noop = function() end
  local cases = {
    not_table = "x",
    bad_to = { to = "sd", name = "a" },
    no_to = { name = "a" },
    no_name = { to = "files" },
    files_name = { to = "files", name = "a/b" },
    storage_name = { to = "storage", name = "Bad" },
    storage_dot = { to = "storage", name = ".x" },
  }
  for label, save in pairs(cases) do
    T.call(label, function() return N:request({ url = "https://s.test/ok", save = save }, noop) end)
  end
end
