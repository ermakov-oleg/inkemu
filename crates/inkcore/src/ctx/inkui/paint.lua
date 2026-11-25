-- ctx.ui (§16): drawing shared by the controls: the button face, font shrinking, badges and the disabled dots.

local TIERS = { "large", "medium", "small" }

function is_font(f)
  return type(f) == "table" and type(f.line_height) == "number"
end

-- `font`, or the largest smaller system font the text fits in, down to line height `n` (then the smallest tried)
function shrink(font, n, fits)
  if n == nil or not is_font(font) or fits(font) then
    return font
  end
  local last = font
  for _, name in ipairs(TIERS) do
    local f = host.fonts[name]
    if f.line_height < font.line_height and f.line_height >= n then
      last = f
      if fits(f) then
        return f
      end
    end
  end
  return last
end

-- the measure options of a text control: only the 2.16 ones it set
function measure_opts(c)
  if c.bold ~= nil or c.tabular ~= nil then
    return { bold = c.bold, tabular = c.tabular }
  end
end

-- One line of `text` in the box `x y w h`, cut with an ellipsis: in the font of `o` or, for a string, a smaller one
-- down to o.font_min that fits the width. `o` holds font, font_min, align, color, bold and tabular; `color` nil leaves
-- the colour to the canvas.
function one_line(g, text, x, y, w, h, o)
  local font = o.font
  if type(text) == "string" then
    font = shrink(font, o.font_min, function(f)
      return f:measure(text, measure_opts(o)) <= w
    end)
  end
  g:text_box(x, y, w, h, text, {
    align = o.align, color = o.color, font = font, overflow = "ellipsis", valign = "middle", wrap = "none",
    bold = o.bold, tabular = o.tabular,
  })
end

-- The text of a button-like box `c` (text font font_min align bold tabular) in the text area `x y w h`, centred
-- unless `align` says otherwise; none when it is nil or empty.
function face_text(g, c, x, y, w, h, color)
  if c.text == nil or c.text == "" then
    return
  end
  one_line(g, c.text, x, y, w, h, {
    font = c.font or host.fonts.medium, font_min = c.font_min, align = c.align or "center", color = color,
    bold = c.bold, tabular = c.tabular,
  })
end

-- the 2 px focus frame 3 px outside the box, rounded by radius + 3 when `r` is a positive number
function focus_frame(g, x, y, w, h, r)
  g:rect(x - 3, y - 3, w + 6, h + 6, { line_width = 2, radius = type(r) == "number" and r > 0 and r + 3 or 0 })
end

-- A button-like box `c` (x y w h radius border selected emphasis skin, and the text fields): the box, the focus ring
-- and the single-line text. Filled when selected, emphasized or focused in the fill style; a skin draws it instead.
function button_face(g, c, look)
  if c.skin then
    return skin_button(g, c, look)
  end
  local fill = c.selected or c.emphasis or (look.focused and not look.outline)
  local ring = look.focused and (c.selected or c.emphasis or look.outline)
  local r = c.radius or 0
  if c.border or fill then
    g:rect(c.x, c.y, c.w, c.h, { fill = fill, radius = r })
  end
  if ring then
    focus_frame(g, c.x, c.y, c.w, c.h, r)
  end
  face_text(g, c, c.x, c.y, c.w, c.h, fill and 0 or 15)
end

-- A button whose face is an image scaled to the box: inverted when selected, the border and the focus frame over it,
-- the text on top. Focus never fills.
function image_face(g, c, look)
  local r = c.radius or 0
  g:image(c.image, c.x, c.y, { w = c.w, h = c.h, invert = c.selected })
  if c.border then
    g:rect(c.x, c.y, c.w, c.h, { radius = r })
  end
  if look.focused then
    focus_frame(g, c.x, c.y, c.w, c.h, r)
  end
  face_text(g, c, c.x, c.y, c.w, c.h, nil)
end

-- white dots over a disabled control `c`, unless its skin has a disabled look; a canvas without `pattern` (the
-- capability is off) gets none
function dots(g, c)
  if not c.enabled and g.pattern ~= nil and not (c.skin and c.skin.disabled) then
    g:pattern(c.x, c.y, c.w, c.h, { color = 0, density = 50, kind = "dot" })
  end
end

-- the badge in the top-right corner of `c`, a paper ring around an ink capsule
function badge(g, c)
  local text = c.badge
  if text == nil or text == "" then
    return
  end
  local small = host.fonts.small
  local bh = small.line_height + 4
  local bw = math.max(bh, small:measure(text) + 8)
  local bx, by = c.x + c.w - 4 - bw, c.y + 4
  g:rect(bx - 2, by - 2, bw + 4, bh + 4, { color = 0, fill = true, radius = bh // 2 + 2 })
  g:rect(bx, by, bw, bh, { color = 15, fill = true, radius = bh // 2 })
  g:text_box(bx, by, bw, bh, text, {
    align = "center", color = 0, font = small, overflow = "ellipsis", valign = "middle", wrap = "none",
  })
end
