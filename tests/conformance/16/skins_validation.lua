local T = require("conformance")

-- a 3 x 3 checkerboard
local PNG = "\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x03\x00\x00\x00\x03\x08\x00\x00\x00\x00\x73\x43\xea\x63\x00\x00\x00\x0f\x49\x44\x41\x54\x78\xda\x63\x60\xf8\xcf\x00\x44\x60\x02\x00\x17\xf4\x03\xfd\x5c\x20\x9c\x4b\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82"

local img

-- "ok" when create takes the skin `s` (named `name`, default "x"), else its error
local function try(s, name)
  local u, e = ctx.ui:create({ skins = { [name or "x"] = s } })
  return u and "ok" or e
end

-- what ui:skin returns for the skin `s` drawn at 0, 0, 100 x 50: its text colour and text area
local function area(g, s)
  local ui = assert(ctx.ui:create({ skins = { x = s } }))
  return { ui:skin(g, "x", 0, 0, 100, 50) }
end

function on_draw(ctx, g)
  g:clear(0)
  img = assert(ctx.image:decode(PNG))

  -- edge and pad: one value, or 1 to 4 in CSS order (top, right, bottom, left); pad defaults to edge
  T.value("css_order", {
    four = area(g, { image = img, pad = { 1, 2, 3, 4 } }),
    three = area(g, { image = img, pad = { 1, 2, 3 } }),
    two = area(g, { image = img, pad = { 1, 2 } }),
    one = area(g, { image = img, pad = { 5 } }),
    number = area(g, { image = img, pad = 5 }),
    from_edge = area(g, { image = img, edge = { 1, 0 }, text_color = 7 }),
  })

  -- left + right must be below the image width and top + bottom below its height; pad is not held to the image
  T.value("fit", {
    edge_1 = try({ image = img, edge = 1 }),
    left_right_3 = try({ image = img, edge = { 0, 1, 0, 2 } }),
    top_bottom_3 = try({ image = img, edge = { 2, 0, 1 } }),
    top_bottom_2 = try({ image = img, edge = { 1, 0, 1, 0 } }),
    big_pad = try({ image = img, pad = { 9, 10 } }),
  })

  T.value("bad_values", {
    no_image = try({ edge = 1 }),
    not_image = try({ image = { width = 3, height = 3 } }),
    edge_256 = try({ image = img, edge = 256 }),
    edge_neg = try({ image = img, edge = -1 }),
    edge_frac = try({ image = img, edge = 0.5 }),
    edge_empty = try({ image = img, edge = {} }),
    edge_five = try({ image = img, edge = { 0, 0, 0, 0, 0 } }),
    pad_256 = try({ image = img, pad = 256 }),
    text_color_16 = try({ image = img, text_color = 16 }),
    empty_name = try({ image = img }, ""),
    not_a_table = try(5),
  })

  -- a variant takes the skin's fields it does not set and is checked with its own image and edge
  T.value("variants", {
    own_edge_too_wide = try({ image = img, selected = { edge = { 0, 2 } } }),
    inherited_edge_fits = try({ image = img, edge = 1, focused = { image = img } }),
    not_a_table = try({ image = img, disabled = 5 }),
    bad_image = try({ image = img, emphasis = { image = 5 } }),
    unknown_ignored = try({ image = img, hover = 5 }),
    text_color = area(g, { image = img, text_color = 3, selected = { text_color = 9 } }),
  })

  -- a skin name a control uses must be registered, and a button cannot have both a skin and an image
  local ui = assert(ctx.ui:create({ skins = { x = { image = img } } }))
  ui:begin_frame(g)
  T.value("use", {
    unknown = select(2, ui:button("a", { x = 0, y = 0, w = 50, h = 30, skin = "nope" })),
    with_image = select(2, ui:button("b", { x = 0, y = 0, w = 50, h = 30, skin = "x", image = img })),
    registered = select("#", ui:button("c", { x = 0, y = 0, w = 50, h = 30, skin = "x" })),
    ui_skin_unknown = select(2, ui:skin(g, "nope", 0, 0, 10, 10)),
  })
  ui:end_frame()
end
