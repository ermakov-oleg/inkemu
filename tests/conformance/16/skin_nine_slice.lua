local T = require("conformance")
local L = require("canvas_log")

-- a 3 x 3 checkerboard: black corners and middle, white edges
local PNG = "\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x03\x00\x00\x00\x03\x08\x00\x00\x00\x00\x73\x43\xea\x63\x00\x00\x00\x0f\x49\x44\x41\x54\x78\xda\x63\x60\xf8\xcf\x00\x44\x60\x02\x00\x17\xf4\x03\xfd\x5c\x20\x9c\x4b\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82"

local ui, img, gone

function on_load(ctx)
  img = assert(ctx.image:decode(PNG))
  gone = assert(ctx.image:decode(PNG))
  ui = ctx.ui:create({ skins = { check = { image = img, edge = 1 }, gone = { image = gone, edge = 1 } } })
  gone:release()
end

local function count(log, op)
  local n = 0
  for _, c in ipairs(log) do
    if string.sub(c, 1, #op + 1) == op .. "(" then n = n + 1 end
  end
  return n
end

function on_draw(ctx, g)
  g:clear(0)
  local font = ctx.fonts.medium

  -- 30 x 30: corners at their own size, edges stretched one way, the middle both ways, one image call per block
  ui:begin_frame(g)
  ui:button("b", { x = 100, y = 100, w = 30, h = 30, skin = "check" })
  local log = L.start(g)
  ui:end_frame()
  T.value("stretched", {
    calls = #log,
    images = count(log, "image"),
    first = log[1] == L.call("image", img, 100, 100, { h = 1, invert = false, src = { h = 1, w = 1, x = 0, y = 0 }, w = 1 }),
    middle = log[5] == L.call("image", img, 101, 101, { h = 28, invert = false, src = { h = 1, w = 1, x = 1, y = 1 }, w = 28 }),
  })

  -- 1 x 2: the left and right edges share the width (1 * 1 // 2 = 0 for the left), the height fits both rows
  -- exactly, so only the two right corners are drawn
  log = L.start(g)
  ui:skin(g, "check", 200, 100, 1, 2)
  T.value("shrunk", L.text(log) == L.call("image", img, 200, 100, { h = 1, invert = false, src = { h = 1, w = 1, x = 2, y = 0 }, w = 1 })
    .. " " .. L.call("image", img, 200, 101, { h = 1, invert = false, src = { h = 1, w = 1, x = 2, y = 2 }, w = 1 }))

  -- a released skin image is refused by the canvas, so nothing of it shows (see the pixels); the text still follows
  ui:begin_frame(g)
  ui:button("r", { x = 300, y = 100, w = 60, h = 30, skin = "gone", text = "Hi" })
  log = L.start(g)
  ui:end_frame()
  local text = log[#log] == L.call("text_box", 301, 101, 58, 28, "Hi", {
    align = "center", color = 15, font = font, overflow = "ellipsis", valign = "middle", wrap = "none",
  })
  T.value("released", { images = count(log, "image"), refused = select(2, g:image(gone, 0, 0)), text = text })

  -- the last frame stays on the screen
  ui:begin_frame(g)
  ui:button("b", { x = 100, y = 100, w = 30, h = 30, skin = "check" })
  ui:button("r", { x = 300, y = 100, w = 60, h = 30, skin = "gone", text = "Hi" })
  ui:end_frame()
  ui:skin(g, "check", 200, 100, 1, 2)
end
