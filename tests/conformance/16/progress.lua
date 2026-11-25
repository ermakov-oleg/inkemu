local T = require("conformance")
local L = require("canvas_log")

-- a 3 x 3 checkerboard: black corners and middle, white edges
local PNG = "\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x03\x00\x00\x00\x03\x08\x00\x00\x00\x00\x73\x43\xea\x63\x00\x00\x00\x0f\x49\x44\x41\x54\x78\xda\x63\x60\xf8\xcf\x00\x44\x60\x02\x00\x17\xf4\x03\xfd\x5c\x20\x9c\x4b\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82"

local ui, img, canvas

function on_load(ctx)
  img = assert(ctx.image:decode(PNG))
  ui = ctx.ui:create({ skins = { check = { image = img, edge = 1 } } })
end

-- the calls end_frame draws for one progress bar `o` (the box is added)
local function drawn(g, o)
  local t = { x = 18, y = 100, w = 444, h = 24 }
  for k, v in pairs(o) do t[k] = v end
  ui:begin_frame(g)
  ui:progress("p", t)
  local log = L.start(g)
  ui:end_frame()
  return log
end

-- the frame of a 444 x 24 bar at 18, 100 and its solid fill for `value`, computed here as §16 states it
local function solid(value)
  local out = { L.call("rect", 18, 100, 444, 24, { line_width = 2 }) }
  local fw = math.floor((444 - 4) * math.max(0, math.min(1, value)) + 0.5)
  if fw > 0 then
    out[2] = L.call("rect", 20, 102, fw, 20, { fill = true })
  end
  return table.concat(out, " ")
end

local function try(name, o)
  T.call(name, function()
    ui:begin_frame(canvas)
    local r = table.pack(ui:progress("p", o))
    ui:end_frame()
    return table.unpack(r, 1, r.n)
  end)
end

local function tap(x, y)
  return { type = "pointer", phase = "up", x = x, y = y, down_x = x, down_y = y, gesture = "tap", time_ms = 0 }
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then
    return
  end
  done = true
  canvas = g

  T.value("solid_0_4", L.text(drawn(g, { value = 0.4 })) == solid(0.4))
  T.value("solid_third", L.text(drawn(g, { value = 1 / 3 })) == solid(1 / 3))
  T.value("solid_clamped_high", L.text(drawn(g, { value = 1.5 })) == solid(1))
  T.value("solid_clamped_low", L.text(drawn(g, { value = -0.2 })) == solid(0))
  T.value("solid_inf", L.text(drawn(g, { value = math.huge })) == solid(1))
  -- `enabled` changes nothing: no dots
  T.value("solid_disabled", L.text(drawn(g, { value = 0.5, enabled = false })) == solid(0.5))
  -- a box 1 px tall has no room inside its frame
  T.value("too_flat", L.text(drawn(g, { value = 1, h = 1 }))
    == L.call("rect", 18, 100, 444, 1, { line_width = 2 }))

  -- 12 x 6 at 3, 5 half full: the filled area is 4 x 2 at 5, 7; the band touching its bottom first, then the one
  -- in the top left corner
  T.value("stripe_tiny", L.text(drawn(g, { x = 3, y = 5, w = 12, h = 6, value = 0.5, style = "stripe" }))
    == table.concat({
      L.call("rect", 3, 5, 12, 6, { line_width = 2 }),
      L.call("polygon", { 7, 9, 9, 7, 9, 9 }, { fill = true }),
      L.call("polygon", { 7, 7, 5, 9, 5, 7 }, { fill = true }),
    }, " "))
  -- a full bar 432 wide: the frame, then a few joined polygons of at most 128 points each
  local log = drawn(g, { x = 18, y = 100, w = 432, h = 24, value = 1, style = "stripe" })
  local within = true
  for i = 2, #log do
    -- the log shows `polygon({1=x,2=y,...},{fill=true})`: one `=` per coordinate and one for `fill`
    local _, n = string.gsub(log[i], "=", "")
    within = within and string.sub(log[i], 1, 8) == "polygon(" and (n - 1) // 2 <= 128
  end
  T.value("stripe_432", { calls = #log, within = within })

  -- a skin without a selected look: the whole bar in the normal look, then its first 15 px (30 * 0.5) inverted;
  -- the middle column is cut to 14 of its 28 px and keeps that share of its source pixel
  log = drawn(g, { x = 100, y = 300, w = 30, h = 30, value = 0.5, skin = "check" })
  T.value("skin", {
    calls = #log,
    lead_corner = log[10] == L.call("image", img, 100, 300,
      { h = 1, invert = true, src = { h = 1, w = 1, x = 0, y = 0 }, w = 1 }),
    lead_middle = log[11] == L.call("image", img, 101, 300,
      { h = 1, invert = true, src = { h = 1, w = 1, x = 1, y = 0 }, w = 14 }),
  })

  try("nan", { x = 0, y = 0, w = 100, h = 20, value = 0 / 0 })
  try("string_value", { x = 0, y = 0, w = 100, h = 20, value = "x" })
  try("no_value", { x = 0, y = 0, w = 100, h = 20 })
  try("bad_style", { x = 0, y = 0, w = 100, h = 20, value = 0.5, style = "dots" })
  try("bad_skin", { x = 0, y = 0, w = 100, h = 20, value = 0.5, skin = "nope" })
  try("enabled_not_checked", { x = 0, y = 0, w = 100, h = 20, value = 0.5, enabled = "x" })

  -- a progress bar over a button is not hit: the tap reaches the button
  ui:begin_frame(g)
  ui:button("under", { x = 18, y = 100, w = 444, h = 24, text = "under" })
  ui:progress("over", { x = 18, y = 100, w = 444, h = 24, value = 0.5 })
  ui:end_frame()
  T.call("tap_through", function() return ui:handle_event(tap(100, 110)) end)
end
