-- ctx.ui (§16, 2.14): composite controls, made of parts that are hit and focused one at a time, and `radio`.
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

local function skinned_radio(g, c, look, font)
  for _, t in ipairs(part_targets(c)) do
    local focused = look.focused and look.part == t.part
    local st = { focused = focused, selected = t.part == c.value, enabled = c.enabled }
    local tc, tx, ty, tw, th, ring = skin_face(g, c.skin, t.x, t.y, t.w, t.h, st)
    local item = c.items[t.part]
    local f = shrink(font, c.font_min, function(f)
      return f:measure(item, measure_opts(c)) <= tw
    end)
    g:text_box(tx, ty, tw, th, item, {
      align = c.align or "left", color = tc, font = f, overflow = "ellipsis", valign = "middle", wrap = "none",
      bold = c.bold,
    })
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
      local f = shrink(font, c.font_min, function(f)
        return f:measure(item, measure_opts(c)) <= tw
      end)
      g:text_box(t.x + s + 8, t.y, tw, t.h, item, {
        align = c.align or "left", color = 15, font = f, overflow = "ellipsis", valign = "middle", wrap = "none",
        bold = c.bold,
      })
      if look.focused and look.part == t.part then
        g:rect(t.x - 3, t.y - 3, t.w + 6, t.h + 6, { line_width = 2, radius = 0 })
      end
    end
    dots(g, c)
  end,
})

methods.radio = control_method("radio")
