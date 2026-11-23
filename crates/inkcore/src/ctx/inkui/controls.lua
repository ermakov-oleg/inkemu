-- ctx.ui (§16): label, button, region and the spacer child.

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
  check = function(c, o)
    if not (check_focusable(c, o) and optional(o.emphasis, "boolean") and optional(o.border, "boolean")) then
      return false
    end
    if not (check_font_min(c, o) and check_text_level(c, o)) then
      return false
    end
    if o.image ~= nil or o.skin ~= nil then
      return false, "ui:button image and skin are not implemented in inkemu yet"
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
    local _, err = declare(state_of(self, name), name, id, o, 3)
    if err then
      return nil, err
    end
  end
end
