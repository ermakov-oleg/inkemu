local T = require("conformance")

function on_load(ctx)
  T.call("list_empty", function()
    local list = ctx.files:list()
    return type(list), #list
  end)
end
