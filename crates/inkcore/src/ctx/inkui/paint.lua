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

-- A button-like box `c` (x y w h radius border selected emphasis text font font_min align bold tabular): the box,
-- the focus ring and the single-line text. Filled when selected, emphasized or focused in the fill style.
function button_face(g, c, look)
  local fill = c.selected or c.emphasis or (look.focused and not look.outline)
  local ring = look.focused and (c.selected or c.emphasis or look.outline)
  local r = c.radius or 0
  if c.border or fill then
    g:rect(c.x, c.y, c.w, c.h, { fill = fill, radius = r })
  end
  if ring then
    local outer = type(r) == "number" and r > 0 and r + 3 or 0
    g:rect(c.x - 3, c.y - 3, c.w + 6, c.h + 6, { line_width = 2, radius = outer })
  end
  if c.text == nil or c.text == "" then
    return
  end
  local font = c.font or host.fonts.medium
  if type(c.text) == "string" then
    font = shrink(font, c.font_min, function(f)
      return f:measure(c.text, measure_opts(c)) <= c.w
    end)
  end
  g:text_box(c.x, c.y, c.w, c.h, c.text, {
    align = c.align or "center", color = fill and 0 or 15, font = font, overflow = "ellipsis",
    valign = "middle", wrap = "none", bold = c.bold, tabular = c.tabular,
  })
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
    local outer = type(r) == "number" and r > 0 and r + 3 or 0
    g:rect(c.x - 3, c.y - 3, c.w + 6, c.h + 6, { line_width = 2, radius = outer })
  end
  if c.text == nil or c.text == "" then
    return
  end
  local font = c.font or host.fonts.medium
  if type(c.text) == "string" then
    font = shrink(font, c.font_min, function(f)
      return f:measure(c.text, measure_opts(c)) <= c.w
    end)
  end
  g:text_box(c.x, c.y, c.w, c.h, c.text, {
    align = c.align or "center", font = font, overflow = "ellipsis", valign = "middle", wrap = "none",
    bold = c.bold, tabular = c.tabular,
  })
end

-- white dots over a disabled control; a canvas without `pattern` (the capability is off) gets none
function dots(g, c)
  if not c.enabled and g.pattern ~= nil then
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
