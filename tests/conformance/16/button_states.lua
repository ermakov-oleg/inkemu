local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

local function want(y, fill, radius, text, o)
  local calls = {}
  if fill ~= nil then calls[1] = L.call("rect", 20, y, 200, 56, { fill = fill, radius = radius }) end
  local opts = { align = "center", color = fill and 0 or 15, font = ctx.fonts.medium, overflow = "ellipsis",
    valign = "middle", wrap = "none" }
  for k, v in pairs(o or {}) do opts[k] = v end
  calls[#calls + 1] = L.call("text_box", 20, y, 200, 56, text, opts)
  return table.concat(calls, " ")
end

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  ui:button("normal", { x = 20, y = 20, w = 200, h = 56, text = "N" })
  ui:button("round", { x = 20, y = 90, w = 200, h = 56, text = "R", radius = 8 })
  ui:button("selected", { x = 20, y = 160, w = 200, h = 56, text = "S", selected = true })
  ui:button("emphasis", { x = 20, y = 230, w = 200, h = 56, text = "E", emphasis = true })
  ui:button("borderless", { x = 20, y = 300, w = 200, h = 56, text = "B", border = false })
  ui:button("left", { x = 20, y = 370, w = 200, h = 56, text = "L", align = "left", font = ctx.fonts.small })
  local log = L.start(g)
  ui:end_frame()
  local got = {}
  for i = 1, #log do got[i] = log[i] end
  local function take(n)
    local out = table.concat(got, " ", 1, n)
    for _ = 1, n do table.remove(got, 1) end
    return out
  end
  T.value("normal", take(2) == want(20, false, 0, "N"))
  T.value("radius", take(2) == want(90, false, 8, "R"))
  T.value("selected", take(2) == want(160, true, 0, "S"))
  T.value("emphasis", take(2) == want(230, true, 0, "E"))
  T.value("borderless", take(1) == want(300, nil, nil, "B"))
  T.value("align_and_font", take(2) == want(370, false, 0, "L", { align = "left", font = ctx.fonts.small }))
  T.value("calls", L.text(log))
end
