local T = require("conformance")

function on_load(ctx)
  local t = ctx.clock:now()
  T.type("revision_type", t.revision)
  T.value("same_snapshot", ctx.clock:now().second == t.second and ctx.clock:now().revision == t.revision)
end
