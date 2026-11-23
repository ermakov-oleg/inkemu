local T = require("conformance")

function on_load(ctx)
  T.value("device.model", ctx.device.model)
  T.value("screen.width", ctx.screen.width)
  T.value("screen.height", ctx.screen.height)
  T.value("screen.gray_levels", ctx.screen.gray_levels)
  T.value("screen.gray_still", ctx.screen.gray_still)
  T.value("input.keyboard_h", ctx.input.keyboard_h)
end
