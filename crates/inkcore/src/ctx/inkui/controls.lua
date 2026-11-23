-- ctx.ui (§16): label, button, region and the spacer child.

local TIERS = { "large", "medium", "small" }

local function is_font(f)
  return type(f) == "table" and type(f.line_height) == "number"
end

-- nil, or a table whose values are strings
local function check_nav(nav)
  if nav == nil then
    return true
  end
  if type(nav) ~= "table" then
    return false
  end
  for _, v in pairs(nav) do
    if type(v) ~= "string" then
      return false
    end
  end
  return true
end

-- the fields every focusable control takes; false when one has the wrong type
local function check_focusable(c, o)
  if not (optional(o.enabled, "boolean") and optional(o.selected, "boolean") and check_nav(o.nav)) then
    return false
  end
  c.enabled = o.enabled ~= false
  c.selected = o.selected == true
  c.nav = o.nav
  c.badge = o.badge
  return optional(o.badge, "string")
end

local function check_font_min(c, o)
  if o.font_min ~= nil and not (is_integral(o.font_min) and o.font_min >= 8 and o.font_min <= 96) then
    return false
  end
  c.font_min = o.font_min
  return true
end

-- the text options of 2.16, ignored below it; false when one has the wrong type
local function text_level(c, o)
  if not api_at_least(16) then
    return true
  end
  c.bold, c.tabular = o.bold, o.tabular
  return optional(o.bold, "boolean") and optional(o.tabular, "boolean") and optional(o.reserve, "string")
end

local function measure_opts(c)
  if c.bold ~= nil or c.tabular ~= nil then
    return { bold = c.bold, tabular = c.tabular }
  end
end

-- `font`, or the largest smaller system font the text fits in, down to line height `n` (then the smallest tried)
local function shrink(font, n, fits)
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

-- white dots over a disabled control; a canvas without `pattern` (the capability is off) gets none
local function dots(g, c)
  if not c.enabled and g.pattern ~= nil then
    g:pattern(c.x, c.y, c.w, c.h, { color = 0, density = 50, kind = "dot" })
  end
end

-- the badge in the top-right corner, a paper ring around an ink capsule
local function badge(g, c)
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

-- label ----------------------------------------------------------------------------------------------------------

kinds.label = {
  check = function(c, o)
    if not (optional(o.text, "string") and check_font_min(c, o) and text_level(c, o)) then
      return false
    end
    c.text, c.font, c.color, c.align, c.valign = o.text or "", o.font, o.color, o.align, o.valign
    return true
  end,
  paint = function(g, c)
    local font = c.font or host.fonts.medium
    -- without the host's font:layout (§7) there is no way to tell what fits, so the font stays
    if is_font(font) and type(font.layout) == "function" then
      font = shrink(font, c.font_min, function(f)
        local _, _, consumed = f:layout(c.text, c.w, c.h, { overflow = "clip", bold = c.bold, tabular = c.tabular })
        return consumed == #c.text
      end)
    end
    g:text_box(c.x, c.y, c.w, c.h, c.text, {
      font = font, overflow = "ellipsis", color = c.color, align = c.align, valign = c.valign,
      bold = c.bold, tabular = c.tabular,
    })
  end,
}

-- button ---------------------------------------------------------------------------------------------------------

kinds.button = {
  focusable = true,
  check = function(c, o)
    if not (check_focusable(c, o) and optional(o.emphasis, "boolean") and optional(o.border, "boolean")) then
      return false
    end
    if not (check_font_min(c, o) and text_level(c, o)) then
      return false
    end
    if o.image ~= nil or o.skin ~= nil then
      error("ui:button image and skin are not implemented in inkemu yet", 3)
    end
    c.emphasis = o.emphasis == true
    c.border = o.border ~= false
    c.text, c.font, c.radius, c.align = o.text, o.font, o.radius, o.align
    return true
  end,
  paint = function(g, c, look)
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
    if c.text ~= nil then
      local font = c.font or host.fonts.medium
      font = shrink(font, c.font_min, function(f)
        return f:measure(c.text, measure_opts(c)) <= c.w
      end)
      g:text_box(c.x, c.y, c.w, c.h, c.text, {
        align = c.align or "center", color = fill and 0 or 15, font = font, overflow = "ellipsis",
        valign = "middle", wrap = "none", bold = c.bold, tabular = c.tabular,
      })
    end
    badge(g, c)
    dots(g, c)
  end,
}

-- region ---------------------------------------------------------------------------------------------------------

kinds.region = {
  focusable = true,
  check = function(c, o)
    if not (check_focusable(c, o) and optional(o.draw, "function")) then
      return false
    end
    c.draw = o.draw
    return true
  end,
  paint = function(g, c, look)
    if c.draw then
      local st = { focused = look.focused, selected = c.selected, enabled = c.enabled }
      c.draw(g, { x = c.x, y = c.y, w = c.w, h = c.h }, st)
    end
    badge(g, c)
  end,
}

-- a layout child that only takes space
kinds.spacer = { cost = 0, check = function() return true end }

for _, name in ipairs({ "label", "button", "region" }) do
  methods[name] = function(self, id, o)
    return declare(self, name, id, o)
  end
end
