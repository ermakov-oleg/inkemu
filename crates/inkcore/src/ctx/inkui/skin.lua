-- ctx.ui (§16, 2.13): skins. `create` checks them into this shape, and buttons, switches, radio cells, containers
-- and `ui:skin` draw them by nine-slice.
--
-- A checked skin is a look plus `pad` and its state variants:
--   look      { image, iw, ih, edge = { t, r, b, l }, text_color }
--   skin      look + pad = { t, r, b, l } + focused, selected, disabled, emphasis (each a look, or nil when unset)
-- A variant's unset fields come from the skin; `pad` belongs to the skin alone.
--
-- What the controls use:
--   check_skins(skins) -> table|nil         the skins of `create` by name, checked; nil when one is invalid
--   skin_face(g, skin, x, y, w, h, state) -> tc, tx, ty, tw, th, ring
--                                           draws the look for `state` (a table, `{}` for the normal look:
--                                           { focused, selected, enabled, emphasis }, only `true` sets one, only
--                                           `enabled == false` clears that one) and
--                                           returns its text colour, the text area after `pad` and whether the
--                                           focus ring goes around it. It draws no text, no ring and no dots: the
--                                           ring comes after the content, so the caller draws it with skin_ring.
--   skin_ring(g, x, y, w, h)                the 2 px focus ring 3 px outside the box
--   skin_dots(g, skin, c)                   the white dots over disabled `c`, unless the skin has a disabled look
--   skin_pad(skin) -> t, r, b, l
--   nine_slice(g, look, x, y, w, h, invert, cut)
--                                           the look's image cut in nine and stretched over the box; with `cut`,
--                                           only its first `cut` columns of pixels are drawn
--   skin_bar(g, skin, x, y, w, h, lead)     a progress bar: the normal look, then the first `lead` px of the
--                                           selected look (or of the normal look inverted)

local VARIANTS = { "focused", "selected", "disabled", "emphasis" }

-- An edge or pad as { t, r, b, l }: one integer 0..255, or 1 to 4 of them in CSS order; `default` when nil, nil when
-- invalid.
local function sides(v, default)
  if v == nil then
    return default
  end
  if int_in(v, 0, 255) then
    v = { v }
  elseif type(v) ~= "table" or #v < 1 or #v > 4 then
    return nil
  end
  for i = 1, #v do
    if not int_in(v[i], 0, 255) then
      return nil
    end
  end
  local t, r = v[1], v[2] or v[1]
  return { math.tointeger(t), math.tointeger(r), math.tointeger(v[3] or t), math.tointeger(v[4] or r) }
end

-- The look the table `o` sets over `base` (nil for the skin itself), or nil when invalid. The edges must leave at
-- least a pixel of the image between them both ways.
local function look_of_table(o, base)
  local image = o.image
  if image == nil and base then
    image = base.image
  end
  if not is_image(image) then
    return nil
  end
  local edge = sides(o.edge, base and base.edge or { 0, 0, 0, 0 })
  local tc = o.text_color
  if tc == nil then
    tc = base and base.text_color or 15
  elseif int_in(tc, 0, 15) then
    tc = math.tointeger(tc)
  else
    return nil
  end
  local iw, ih = math.tointeger(image.width), math.tointeger(image.height)
  if edge == nil or edge[2] + edge[4] >= iw or edge[1] + edge[3] >= ih then
    return nil
  end
  return { image = image, iw = iw, ih = ih, edge = edge, text_color = tc }
end

function check_skins(skins)
  if type(skins) ~= "table" then
    return nil
  end
  local out = {}
  for name, o in pairs(skins) do
    if type(name) ~= "string" or name == "" or type(o) ~= "table" then
      return nil
    end
    local s = look_of_table(o, nil)
    -- the pad is not held to the image: it only places the text
    local pad = s and sides(o.pad, s.edge)
    if pad == nil then
      return nil
    end
    s.pad = pad
    for _, k in ipairs(VARIANTS) do
      local v = o[k]
      if v ~= nil then
        s[k] = type(v) == "table" and look_of_table(v, s) or nil
        if s[k] == nil then
          return nil
        end
      end
    end
    out[name] = s
  end
  return out
end

function skin_pad(skin)
  local p = skin.pad
  return p[1], p[2], p[3], p[4]
end

-- the two edges and the middle of `size`: edges that do not fit share it in proportion and leave no middle
local function spans(size, a, b)
  if size < a + b then
    a = size * a // (a + b)
    return a, 0, size - a
  end
  return a, size - a - b, b
end

-- A column cut short keeps the share of its source that it shows, rounded to the nearest pixel but at least one
-- (oracle s33).
function nine_slice(g, lk, x, y, w, h, invert, cut)
  local e = lk.edge
  local cols = { spans(w, e[4], e[2]) }
  local rows = { spans(h, e[1], e[3]) }
  local sx, sw = { 0, e[4], lk.iw - e[2] }, { e[4], lk.iw - e[4] - e[2], e[2] }
  local sy, sh = { 0, e[1], lk.ih - e[3] }, { e[1], lk.ih - e[1] - e[3], e[3] }
  local dy = y
  for r = 1, 3 do
    local dx = x
    for c = 1, 3 do
      local dw, srcw = cols[c], sw[c]
      if cut and dx - x + dw > cut then
        dw = math.max(0, cut - (dx - x))
        if dw > 0 then
          srcw = math.max(1, (2 * sw[c] * dw + cols[c]) // (2 * cols[c]))
        end
      end
      if dw > 0 and rows[r] > 0 then
        -- a released image is refused by the canvas without an error, so the text still follows
        g:image(lk.image, dx, dy, {
          w = dw, h = rows[r], invert = invert, src = { x = sx[c], y = sy[r], w = srcw, h = sh[r] },
        })
      end
      dx = dx + cols[c]
    end
    dy = dy + rows[r]
  end
end

-- The look for `st` by priority (disabled, selected, emphasis, focus), whether it is the normal look inverted, and
-- whether the focus ring goes around it: always with focus, unless the focused look itself is drawn.
local function pick(skin, st)
  local focused = st.focused == true
  if st.enabled == false and skin.disabled then
    return skin.disabled, false, focused
  end
  if st.selected == true then
    if skin.selected then
      return skin.selected, false, focused
    end
    return skin, true, focused
  end
  if st.emphasis == true and skin.emphasis then
    return skin.emphasis, false, focused
  end
  if focused and skin.focused then
    return skin.focused, false, false
  end
  return skin, false, focused
end

function skin_face(g, skin, x, y, w, h, st)
  local lk, invert, ring = pick(skin, st)
  nine_slice(g, lk, x, y, w, h, invert)
  local t, r, b, l = skin_pad(skin)
  -- the inverted normal look inverts its text colour too
  local tc = invert and 15 - lk.text_color or lk.text_color
  return tc, x + l, y + t, math.max(1, w - l - r), math.max(1, h - t - b), ring
end

function skin_bar(g, skin, x, y, w, h, lead)
  nine_slice(g, skin, x, y, w, h, false)
  if lead > 0 then
    local lk, invert = pick(skin, { selected = true })
    nine_slice(g, lk, x, y, w, h, invert, math.min(lead, w))
  end
end

function skin_ring(g, x, y, w, h)
  g:rect(x - 3, y - 3, w + 6, h + 6, { line_width = 2, radius = 0 })
end

function skin_dots(g, skin, c)
  if skin.disabled == nil then
    dots(g, c)
  end
end

-- A button with a skin: the look, the text in the area after `pad`, then the focus ring. `radius` and `border` have
-- no effect.
function skin_button(g, c, look)
  local st = { focused = look.focused, selected = c.selected, enabled = c.enabled, emphasis = c.emphasis }
  local tc, tx, ty, tw, th, ring = skin_face(g, c.skin, c.x, c.y, c.w, c.h, st)
  face_text(g, c, tx, ty, tw, th, tc)
  if ring then
    skin_ring(g, c.x, c.y, c.w, c.h)
  end
end

-- `ui:skin(g, name, x, y, w, h, st)`: draws at once on `g`, in a frame or not, with the ring but never the dots
methods.skin = function(self, g, name, x, y, w, h, st)
  local s = state_of(self, "skin")
  local skin = type(name) == "string" and s.skins[name]
  if not (skin and is_canvas(g) and is_integral(x) and is_integral(y) and is_side(w) and is_side(h)) then
    return nil, INVALID
  end
  if st ~= nil and type(st) ~= "table" then
    return nil, INVALID
  end
  x, y, w, h = math.tointeger(x), math.tointeger(y), math.tointeger(w), math.tointeger(h)
  local tc, tx, ty, tw, th, ring = skin_face(g, skin, x, y, w, h, st or {})
  if ring then
    skin_ring(g, x, y, w, h)
  end
  return tc, tx, ty, tw, th
end
