-- ctx.ui (§16, 2.14): composite controls, made of parts that are hit and focused one at a time, and `radio` and
-- `tabs`.
--
-- A composite kind has, besides `check` and `paint`:
--   count(c)            how many parts the control has this frame (each costs one control more)
--   parts(c)            its parts in order, as { part = number, x, y, w, h }, from the control's final box
--   part_enabled(c, n)  optional: false when part `n` is unavailable on its own
--   default_part(c)     the part that focus on the whole control takes
--   activate(c, n)      what activating part `n` returns after the id: an action and the third value
-- `composite(kind)` fills in what makes it one: it takes focus through its parts and costs 1 plus its parts.

-- The part targets of composite `c`, made once per frame from its final box: `id`, `part`, the box, `enabled` and
-- `owner` (the control). They have no `nav`: a composite ignores one.
local function part_targets(c)
  local list = c.part_targets
  if list then
    return list
  end
  local kind = c.kind
  list = {}
  for i, p in ipairs(kind.parts(c)) do
    local on = c.enabled and (kind.part_enabled == nil or kind.part_enabled(c, p.part))
    list[i] = { id = c.id, part = p.part, x = p.x, y = p.y, w = p.w, h = p.h, enabled = on, owner = c }
  end
  c.part_targets = list
  return list
end

local function composite(kind)
  kind.focusable = true
  kind.targets = part_targets
  kind.cost = function(c)
    return 1 + kind.count(c)
  end
  return kind
end

-- Splits `len` into `k` cells with `gap` between neighbours, as { start, size }: start_j = j * gap +
-- floor(j * (len - (k - 1) * gap) / k) for j from 0, each cell stopping `gap` short of the next, the last at `len`.
-- Not measured: a gap wider than `len` counts as `len` (keeping the products in 32 bits) and a cell is never below 0.
function split_cells(len, k, gap)
  gap = math.min(gap, len)
  local free = len - (k - 1) * gap
  local starts = {}
  for j = 0, k - 1 do
    starts[j + 1] = j * gap + j * free // k
  end
  local out = {}
  for j = 1, k do
    local stop = j < k and starts[j + 1] - gap or len
    out[j] = { starts[j], math.max(0, stop - starts[j]) }
  end
  return out
end

-- the 2.16 `bold` of a text control; false when it has the wrong type
local function check_bold(c, o)
  if api_at_least(16) then
    c.bold = o.bold
    return optional(o.bold, "boolean")
  end
  return true
end

local ALIGN = { left = true, center = true, right = true }

-- The one line of a cell in the box `x y w h`: the font of `o` (or a smaller one from c.font_min that fits the width),
-- cut with an ellipsis. `o` holds align, color, font, bold and tabular.
local function cell_text(g, c, text, x, y, w, h, o)
  local f = shrink(o.font, c.font_min, function(f)
    return f:measure(text, { bold = o.bold, tabular = o.tabular }) <= w
  end)
  g:text_box(x, y, w, h, text, {
    align = o.align, color = o.color, font = f, overflow = "ellipsis", valign = "middle", wrap = "none",
    bold = o.bold, tabular = o.tabular,
  })
end

local function skinned_radio(g, c, look, font)
  for _, t in ipairs(part_targets(c)) do
    local focused = look.focused and look.part == t.part
    local st = { focused = focused, selected = t.part == c.value, enabled = c.enabled }
    local tc, tx, ty, tw, th, ring = skin_face(g, c.skin, t.x, t.y, t.w, t.h, st)
    local item = c.items[t.part]
    cell_text(g, c, item, tx, ty, tw, th, { align = c.align or "left", color = tc, font = font, bold = c.bold })
    if ring then
      skin_ring(g, t.x, t.y, t.w, t.h)
    end
  end
  skin_dots(g, c.skin, c)
end

kinds.radio = composite({
  check = function(c, o, st)
    -- `nav` is checked as on other controls, then ignored: a composite takes focus by its parts
    if not (optional(o.enabled, "boolean") and check_nav(o.nav) and check_items(o.items) and check_cell_gap(o.gap)) then
      return false
    end
    local n = #o.items
    if not ((o.value == nil or int_in(o.value, 1, n)) and (o.columns == nil or int_in(o.columns, 1, n))
      and (o.align == nil or ALIGN[o.align]) and check_font_min(c, o) and check_bold(c, o)) then
      return false
    end
    local ok, skin = check_skin(st, o.skin)
    if not ok then
      return false
    end
    c.skin = skin
    c.enabled = o.enabled ~= false
    c.items = table.move(o.items, 1, n, 1, {})
    c.value = o.value and math.tointeger(o.value)
    c.columns = o.columns and math.tointeger(o.columns) or 1
    c.gap = o.gap and math.tointeger(o.gap) or 0
    c.align, c.font = o.align, o.font
    return true
  end,
  count = function(c)
    return #c.items
  end,
  -- filled left to right, then top to bottom; the cells split the box both ways with the same gap
  parts = function(c)
    local n, cols = #c.items, c.columns
    local xs, ys = split_cells(c.w, cols, c.gap), split_cells(c.h, (n + cols - 1) // cols, c.gap)
    local out = {}
    for i = 1, n do
      local col, row = xs[(i - 1) % cols + 1], ys[(i - 1) // cols + 1]
      out[i] = { part = i, x = c.x + col[1], y = c.y + row[1], w = col[2], h = row[2] }
    end
    return out
  end,
  default_part = function(c)
    return c.value or 1
  end,
  activate = function(c, n)
    if n == c.value then
      return "activate", n
    end
    return "change", n
  end,
  -- per cell: a ring of diameter s on the left, the dot of the value item, the text after them, the focus frame;
  -- with a skin, the skin's look in place of the ring and dot, and the text in the area after its pad
  paint = function(g, c, look)
    local font = c.font or host.fonts.medium
    if c.skin then
      return skinned_radio(g, c, look, font)
    end
    for _, t in ipairs(part_targets(c)) do
      local s = math.min(t.h, 32)
      local cx, cy = t.x + s // 2, t.y + t.h // 2
      g:circle(cx, cy, s // 2, { line_width = 2 })
      -- not measured: a cell too small for a dot of diameter s - 10 >= 1 gets none
      if t.part == c.value and s > 10 then
        g:circle(cx, cy, (s - 10) // 2, { fill = true })
      end
      local item, tw = c.items[t.part], math.max(0, t.w - s - 8)
      cell_text(g, c, item, t.x + s + 8, t.y, tw, t.h, { align = c.align or "left", color = 15, font = font, bold = c.bold })
      if look.focused and look.part == t.part then
        g:rect(t.x - 3, t.y - 3, t.w + 6, t.h + 6, { line_width = 2, radius = 0 })
      end
    end
    dots(g, c)
  end,
})

methods.radio = control_method("radio")

local SELECTED_STYLES = { fill = true, outline = true, underline = true }

-- a tabs cell's bold: from 2.16 the selected cell unless the script said otherwise
local function tab_bold(c, selected)
  if not api_at_least(16) then
    return nil
  end
  if c.bold ~= nil then
    return c.bold or nil
  end
  return selected or nil
end

kinds.tabs = composite({
  check = function(c, o, st)
    if not (optional(o.enabled, "boolean") and check_nav(o.nav) and check_items(o.items) and check_cell_gap(o.gap)) then
      return false
    end
    local n = #o.items
    if not ((o.value == nil or int_in(o.value, 1, n)) and (o.align == nil or ALIGN[o.align])
      and optional(o.border, "boolean") and check_font_min(c, o) and check_bold(c, o)) then
      return false
    end
    if api_at_least(16) and o.selected_style ~= nil and not SELECTED_STYLES[o.selected_style] then
      return false
    end
    local ok, skin = check_skin(st, o.skin)
    if not ok then
      return false
    end
    c.skin = skin
    c.enabled = o.enabled ~= false
    c.items = table.move(o.items, 1, n, 1, {})
    c.value = o.value and math.tointeger(o.value)
    c.gap = o.gap and math.tointeger(o.gap) or 0
    c.border = o.border ~= false
    c.style = api_at_least(16) and o.selected_style or "fill"
    c.align, c.font, c.radius = o.align, o.font, o.radius
    return true
  end,
  count = function(c)
    return #c.items
  end,
  parts = function(c)
    local out = {}
    for i, cell in ipairs(split_cells(c.w, #c.items, c.gap)) do
      out[i] = { part = i, x = c.x + cell[1], y = c.y, w = cell[2], h = c.h }
    end
    return out
  end,
  default_part = function(c)
    return c.value or 1
  end,
  activate = function(c, n)
    if n == c.value then
      return "activate", n
    end
    return "change", n
  end,
  -- per cell, in this order: the box (filled when selected in the fill style; focus never fills), the outline or
  -- underline of the selected style, the focus frame, the text. A skin draws its look in place of the box.
  paint = function(g, c, look)
    local font = c.font or host.fonts.medium
    local r = c.radius or 0
    for _, t in ipairs(part_targets(c)) do
      local selected = t.part == c.value
      local focused = look.focused and look.part == t.part
      local bold = tab_bold(c, selected)
      if c.skin then
        local st = { focused = focused, selected = selected, enabled = c.enabled }
        local tc, tx, ty, tw, th, ring = skin_face(g, c.skin, t.x, t.y, t.w, t.h, st)
        cell_text(g, c, c.items[t.part], tx, ty, tw, th, { align = c.align or "center", color = tc, font = font, bold = bold })
        if ring then
          skin_ring(g, t.x, t.y, t.w, t.h)
        end
      else
        local fill = selected and c.style == "fill"
        if c.border or fill then
          g:rect(t.x, t.y, t.w, t.h, { fill = fill, radius = r })
        end
        if selected and c.style == "outline" then
          g:rect(t.x, t.y, t.w, t.h, { line_width = 3, radius = r })
        elseif selected and c.style == "underline" then
          local inset = math.min(8, t.w // 6)
          g:rect(t.x + inset, t.y + t.h - 4, t.w - 2 * inset, 4, { fill = true })
        end
        if focused then
          g:rect(t.x - 3, t.y - 3, t.w + 6, t.h + 6, {
            line_width = 2, radius = type(r) == "number" and r > 0 and r + 3 or 0,
          })
        end
        cell_text(g, c, c.items[t.part], t.x, t.y, t.w, t.h, {
          align = c.align or "center", color = fill and 0 or 15, font = font, bold = bold,
        })
      end
    end
    if c.skin then
      skin_dots(g, c.skin, c)
    else
      dots(g, c)
    end
  end,
})

methods.tabs = control_method("tabs")

-- What stepper and pager share: two button parts at the ends of the box and a text between them. Part 1 goes
-- backwards (minus, previous), part 2 forwards. A part at its end of the range is unavailable: it is drawn with
-- dots, takes no hit and is skipped by the arrows, and the box as a whole keeps its own dots when disabled.
local function check_ends(c, o, st)
  if not (optional(o.enabled, "boolean") and check_nav(o.nav) and optional(o.border, "boolean")
    and optional(o.text, "string")) then
    return false
  end
  if api_at_least(16) and not optional(o.tabular, "boolean") then
    return false
  end
  local ok, skin = check_skin(st, o.skin)
  if not ok then
    return false
  end
  c.skin = skin
  c.enabled = o.enabled ~= false
  c.border = o.border ~= false
  c.text, c.font, c.radius = o.text, o.font, o.radius
  -- the middle text is tabular unless told otherwise; the draw call only gets the key when it is on
  c.tabular = api_at_least(16) and o.tabular ~= false or nil
  return true
end

local function ends_default_part(c)
  return c.kind.part_enabled(c, 1) and 1 or c.kind.part_enabled(c, 2) and 2 or 1
end

-- the button-like record of part `t` of `c`, with `text` on it
local function end_part(c, t, text)
  return {
    x = t.x, y = t.y, w = t.w, h = t.h, border = c.border, radius = c.radius, skin = c.skin, font = c.font,
    text = text, enabled = t.enabled, selected = false, emphasis = false,
  }
end

-- Paints the parts of `c` in order: `face(part_record, look, t)` draws one (box, focus, content), then the dots of a
-- part that is only unavailable; then the middle text and, last, the dots of a disabled box.
local function paint_ends(g, c, look, middle, face)
  for _, t in ipairs(part_targets(c)) do
    local p = end_part(c, t, t.part == 1 and "<" or ">")
    face(p, { focused = look.focused and look.part == t.part, outline = look.outline }, t)
    if c.enabled and not t.enabled then
      if c.skin then
        skin_dots(g, c.skin, { x = t.x, y = t.y, w = t.w, h = t.h, enabled = false })
      else
        dots(g, { x = t.x, y = t.y, w = t.w, h = t.h, enabled = false })
      end
    end
  end
  if middle.w > 0 and middle.text ~= "" then
    cell_text(g, c, middle.text, middle.x, middle.y, middle.w, middle.h, {
      align = "center", color = 15, font = c.font or host.fonts.medium, tabular = c.tabular,
    })
  end
  if c.skin then
    skin_dots(g, c.skin, c)
  else
    dots(g, c)
  end
end

-- the plus or minus sign centred in the area `t` of a part (the part, or its skin's text area): a bar of at least 6 px, about 9/20 of the part, 3 px thick
local function draw_sign(g, t, plus, color)
  local len = math.max(6, math.min(t.w, t.h) * 9 // 20)
  local cx, cy = t.x + t.w // 2, t.y + t.h // 2
  g:rect(cx - len // 2, cy - 1, len, 3, { color = color, fill = true })
  if plus then
    g:rect(cx - 1, cy - len // 2, 3, len, { color = color, fill = true })
  end
end

kinds.stepper = composite({
  check = function(c, o, st)
    if not check_ends(c, o, st) then
      return false
    end
    local lo, hi, v = o.min, o.max, o.value
    if not (is_integral(lo) and is_integral(hi) and lo <= hi and int_in(v, lo, hi)
      and (o.step == nil or int_in(o.step, 1, math.maxinteger)) and c.w >= 2 * c.h) then
      return false
    end
    c.min, c.max, c.value = math.tointeger(lo), math.tointeger(hi), math.tointeger(v)
    c.step = o.step and math.tointeger(o.step) or 1
    return true
  end,
  count = function()
    return 2
  end,
  parts = function(c)
    return {
      { part = 1, x = c.x, y = c.y, w = c.h, h = c.h },
      { part = 2, x = c.x + c.w - c.h, y = c.y, w = c.h, h = c.h },
    }
  end,
  part_enabled = function(c, n)
    if n == 1 then
      return c.value > c.min
    end
    return c.value < c.max
  end,
  default_part = ends_default_part,
  activate = function(c, n)
    if n == 1 then
      return "change", math.max(c.min, c.value - c.step)
    end
    return "change", math.min(c.max, c.value + c.step)
  end,
  paint = function(g, c, look)
    local middle = {
      x = c.x + c.h, y = c.y, w = c.w - 2 * c.h, h = c.h, text = c.text or tostring(c.value),
    }
    paint_ends(g, c, look, middle, function(p, lk, t)
      -- the sign goes over the box and the focus frame; focus fills the box, so the sign turns to paper
      p.text = nil
      if c.skin then
        local st = { focused = lk.focused, enabled = t.enabled }
        local tc, tx, ty, tw, th, ring = skin_face(g, c.skin, t.x, t.y, t.w, t.h, st)
        draw_sign(g, { x = tx, y = ty, w = tw, h = th }, t.part == 2, tc)
        if ring then
          skin_ring(g, t.x, t.y, t.w, t.h)
        end
      else
        button_face(g, p, lk)
        draw_sign(g, t, t.part == 2, lk.focused and not lk.outline and 0 or 15)
      end
    end)
  end,
})

kinds.pager = composite({
  check = function(c, o, st)
    if not check_ends(c, o, st) then
      return false
    end
    local pages, page, bw = o.pages, o.page, o.button_w
    if not (is_integral(pages) and pages >= 1 and int_in(page, 1, pages)
      and (bw == nil or int_in(bw, 1, c.w // 2))) then
      return false
    end
    c.pages, c.page = math.tointeger(pages), math.tointeger(page)
    -- not measured: the default button width is h, but never more than half the box
    c.button_w = bw and math.tointeger(bw) or math.min(c.h, c.w // 2)
    return true
  end,
  count = function()
    return 2
  end,
  parts = function(c)
    local bw = c.button_w
    return {
      { part = 1, x = c.x, y = c.y, w = bw, h = c.h },
      { part = 2, x = c.x + c.w - bw, y = c.y, w = bw, h = c.h },
    }
  end,
  part_enabled = function(c, n)
    if n == 1 then
      return c.page > 1
    end
    return c.page < c.pages
  end,
  default_part = ends_default_part,
  activate = function(c, n)
    return "change", n == 1 and c.page - 1 or c.page + 1
  end,
  paint = function(g, c, look)
    local middle = {
      x = c.x + c.button_w, y = c.y, w = c.w - 2 * c.button_w, h = c.h, text = c.text or (c.page .. "/" .. c.pages),
    }
    paint_ends(g, c, look, middle, function(p, lk)
      p.tabular = nil
      button_face(g, p, lk)
    end)
  end,
})

methods.stepper = control_method("stepper")
methods.pager = control_method("pager")
