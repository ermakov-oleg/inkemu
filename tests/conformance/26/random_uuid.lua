local T = require("conformance")

function on_load(ctx)
  local u = ctx.random:uuid()
  T.value("length", #u)
  T.value("version4_lowercase", string.match(u, "^%x%x%x%x%x%x%x%x%-%x%x%x%x%-4%x%x%x%-[89ab]%x%x%x%-%x%x%x%x%x%x%x%x%x%x%x%x$") ~= nil)
  T.value("no_uppercase", u == string.lower(u))
  T.value("differ", u ~= ctx.random:uuid())
end
