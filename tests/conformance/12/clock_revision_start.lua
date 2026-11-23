local T = require("conformance")

function on_load(ctx)
  T.value("revision", ctx.clock:now().revision)
end
