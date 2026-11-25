-- ctx.ui (§16): label, button, switch, region and the spacer child.

kinds.label = {
  check = function(c, o)
    if not (check_font_min(c, o) and check_text_level(c, o)) then
      return false
    end
    -- any text is accepted and handed to text_box as it is; the host decides about a non-string
    if o.text == nil then
      c.text = ""
    else
      c.text = o.text
    end
    c.font, c.color, c.align, c.valign = o.font, o.color, o.align, o.valign
    return true
  end,
  paint = function(g, c)
    local font = c.font or host.fonts.medium
    if type(c.text) == "string" and is_font(font) and type(font.layout) == "function" then
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

kinds.button = {
  focusable = true,
  check = function(c, o, st)
    if not (check_focusable(c, o) and optional(o.emphasis, "boolean") and optional(o.border, "boolean")) then
      return false
    end
    if not (check_font_min(c, o) and check_text_level(c, o)) or (o.image ~= nil and o.skin ~= nil) then
      return false
    end
    local ok, unsupported = check_skin(st, o.skin, "ui:button")
    if not ok then
      return false, unsupported
    end
    if o.image ~= nil then
      return false, "ui:button image is not implemented in inkemu yet"
    end
    c.emphasis = o.emphasis == true
    c.border = o.border ~= false
    c.text, c.font, c.radius, c.align = o.text, o.font, o.radius, o.align
    return true
  end,
  paint = function(g, c, look)
    button_face(g, c, look)
    badge(g, c)
    dots(g, c)
  end,
}

kinds.switch = {
  focusable = true,
  check = function(c, o, st)
    -- unlike a button's, `selected` is not type-checked (any value is on unless nil or false); a badge is ignored
    if not (optional(o.enabled, "boolean") and check_nav(o.nav) and optional(o.note, "string")) then
      return false
    end
    if api_at_least(16) and not optional(o.bold, "boolean") then
      return false
    end
    local ok, unsupported = check_skin(st, o.skin, "ui:switch")
    if not ok then
      return false, unsupported
    end
    c.enabled = o.enabled ~= false
    c.selected = o.selected ~= nil and o.selected ~= false
    c.nav = o.nav
    c.text, c.font, c.radius, c.note = o.text, o.font, o.radius, o.note
    c.bold = api_at_least(16) and o.bold ~= false or nil
    return true
  end,
  paint = function(g, c, look)
    local p = c.w > 32 and 8 or 0
    local th = math.min(c.h, 34)
    local tw = math.min(c.w - 2 * p, 2 * th - 4)
    th = math.min(th, tw)
    local cx = c.x + c.w - p - tw
    local cy = c.y + (c.h - th) // 2
    g:rect(cx, cy, tw, th, { fill = c.selected, line_width = 2, radius = th // 2 })
    if c.selected then
      g:circle(cx + tw - th // 2, cy + th // 2, th // 2 - 5, { color = 0, fill = true })
    else
      g:circle(cx + th // 2, cy + th // 2, th // 2 - 6, { fill = true })
    end
    local tx = c.x + p
    local room = cx - 12 - tx
    if room > 0 then
      local font = c.font or host.fonts.medium
      local note = c.note ~= nil and c.note ~= ""
      if note then
        -- two pixels clear of the middle line on either side
        local upper = c.h // 2 - 2
        local lower = c.y + c.h // 2 + 2
        g:text_box(tx, c.y, room, upper, c.text or "", {
          font = font, overflow = "ellipsis", valign = "bottom", wrap = "none", bold = c.bold,
        })
        g:text_box(tx, lower, room, c.y + c.h - lower, c.note, {
          color = 10, font = host.fonts.small, overflow = "ellipsis", valign = "top", wrap = "none",
        })
      else
        g:text_box(tx, c.y, room, c.h, c.text or "", {
          font = font, overflow = "ellipsis", valign = "middle", wrap = "none", bold = c.bold,
        })
      end
    end
    if look.focused then
      local r = c.radius
      g:rect(c.x - 3, c.y - 3, c.w + 6, c.h + 6, {
        line_width = 2, radius = type(r) == "number" and r > 0 and r + 3 or 0,
      })
    end
    dots(g, c)
  end,
}

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

for _, name in ipairs({ "label", "button", "switch", "region" }) do
  methods[name] = control_method(name)
end
