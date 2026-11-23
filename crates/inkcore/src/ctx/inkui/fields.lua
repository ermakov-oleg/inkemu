-- ctx.ui (§16): checks of control fields shared by every kind of control.

INVALID = "invalid_argument"

function is_integral(v)
  return type(v) == "number" and v == v and v - v == 0 and math.floor(v) == v
end

function is_side(v)
  return is_integral(v) and v >= 1
end

-- nil or the value's Lua type
function optional(v, ty)
  return v == nil or type(v) == ty
end

-- nil, or a table whose values are strings
function check_nav(nav)
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
function check_focusable(c, o)
  if not (optional(o.enabled, "boolean") and optional(o.selected, "boolean") and check_nav(o.nav)) then
    return false
  end
  c.enabled = o.enabled ~= false
  c.selected = o.selected == true
  c.nav = o.nav
  c.badge = o.badge
  return optional(o.badge, "string")
end

function check_font_min(c, o)
  if o.font_min ~= nil and not (is_integral(o.font_min) and o.font_min >= 8 and o.font_min <= 96) then
    return false
  end
  c.font_min = o.font_min
  return true
end

-- the text options of 2.16, ignored below it; false when one has the wrong type
function check_text_level(c, o)
  if not api_at_least(16) then
    return true
  end
  c.bold, c.tabular = o.bold, o.tabular
  return optional(o.bold, "boolean") and optional(o.tabular, "boolean") and optional(o.reserve, "string")
end
