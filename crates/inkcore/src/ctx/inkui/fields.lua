-- ctx.ui (§16): checks of control fields shared by every kind of control.

INVALID = "invalid_argument"

function is_integral(v)
  return type(v) == "number" and v == v and v - v == 0 and math.floor(v) == v
end

function is_side(v)
  return is_integral(v) and v >= 1
end

-- the largest coordinate and size the reader takes (it refuses a child placed past it and a wider ui:layout rect)
MAX_COORD = 32767

-- a control's placed box within those coordinates
function in_range(c)
  return c.x >= -MAX_COORD - 1 and c.x <= MAX_COORD and c.y >= -MAX_COORD - 1 and c.y <= MAX_COORD
    and c.w <= MAX_COORD and c.h <= MAX_COORD
end

function int_in(v, lo, hi)
  return is_integral(v) and v >= lo and v <= hi
end

-- the `items` of tabs and radio: an array of 1..64 strings
function check_items(items)
  if type(items) ~= "table" or #items < 1 or #items > 64 then
    return false
  end
  for i = 1, #items do
    if type(items[i]) ~= "string" then
      return false
    end
  end
  return true
end

-- the `gap` between the cells of tabs and radio: nil or an integer >= 0
function check_cell_gap(gap)
  return gap == nil or (is_integral(gap) and gap >= 0)
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
