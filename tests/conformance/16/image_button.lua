local T = require("conformance")
local L = require("canvas_log")
local ui

-- a 2 x 2 black PNG
local PNG = "\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x02\x00\x00\x00\x02\x08\x00\x00\x00\x00\x57\xdd\x52\xf8\x00\x00\x00\x0b\x49\x44\x41\x54\x78\x9c\x63\x60\x00\x01\x00\x00\x06\x00\x01\xfe\x8c\x67\xc8\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82"

function on_load(ctx)
  ui = ctx.ui:create()
end

local function drawn(g, name, id, o)
  ui:begin_frame(g)
  local a, b = ui[name](ui, id, o)
  local log = L.start(g)
  ui:end_frame()
  return L.text(log), a, b
end

function on_draw(ctx, g)
  g:clear(0)
  local img = assert(ctx.image:decode(PNG))
  local font = ctx.fonts.medium

  -- the image fills the box; selected inverts it; the frame goes 3 px outside, after the image and before the text
  local box = { x = 20, y = 100, w = 96, h = 96, image = img, border = false }
  local function with(extra)
    local o = {}
    for k, v in pairs(box) do o[k] = v end
    for k, v in pairs(extra) do o[k] = v end
    return o
  end
  T.value("plain", drawn(g, "button", "a", with({}))
    == L.call("image", img, 20, 100, { h = 96, invert = false, w = 96 }))
  T.value("selected", drawn(g, "button", "a", with({ selected = true }))
    == L.call("image", img, 20, 100, { h = 96, invert = true, w = 96 }))
  T.value("text", drawn(g, "button", "a", with({ text = "Go" }))
    == L.call("image", img, 20, 100, { h = 96, invert = false, w = 96 }) .. " "
      .. L.call("text_box", 20, 100, 96, 96, "Go", { align = "center", font = font, overflow = "ellipsis",
        valign = "middle", wrap = "none" }))
  T.value("image_control", drawn(g, "image", "i", { x = 20, y = 100, w = 50, h = 30, source = img })
    == L.call("image", img, 20, 100, { h = 30, w = 50 }))
  T.value("not_an_image", {
    number = select(3, drawn(g, "button", "a", with({ image = 5 }))),
    table = select(3, drawn(g, "button", "a", with({ image = { width = 10, height = 10 } }))),
  })

  -- the last frame stays on the screen: the black image over paper
  ui:begin_frame(g)
  ui:button("a", box)
  ui:end_frame()
end
