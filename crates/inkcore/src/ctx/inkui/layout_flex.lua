-- ctx.ui (§16, 2.15): the flex layout of the nodes layout_tree.lua builds: content sizes, the main axis (base,
-- hypothetical size, line breaking, the freeze loop, justify_content), the cross axis, baselines and overflow.
-- Everything is in integers; the splits use cumulative rounding. The helpers layout_grid.lua shares are globals
-- of the library's environment; a grid's or stack's sizes and boxes come from there.

-- the main and the cross axis of a row and a column
local MAIN = { row = "w", column = "h" }
local CROSS = { row = "h", column = "w" }
local SPACE = { ["space-between"] = true, ["space-around"] = true, ["space-evenly"] = true }

function ceil_div(a, b)
  return -((-a) // b)
end

local function clamp(v, lo, hi)
  if hi and v > hi then
    v = hi
  end
  if v < lo then
    v = lo
  end
  return v
end

function bound(n, axis, v)
  if axis == "w" then
    return clamp(v, n.lo_w, n.hi_w)
  end
  return clamp(v, n.lo_h, n.hi_h)
end

-- a container's own padding along `axis`, 0 for any other node
function pad(n, axis)
  if not CONTAINERS[n.kind] then
    return 0
  end
  if axis == "w" then
    return n.pad_l + n.pad_r
  end
  return n.pad_t + n.pad_b
end

function round(v)
  return math.floor(v + 0.5)
end

-- a text that is not a string reached here only where its size does not matter (see check_sizing)
local function width_of(font, text, opts)
  if type(text) ~= "string" or text == "" then
    return 0
  end
  return font:measure(text, opts)
end

-- the widest of the lines of `text`
local function widest_line(font, text, opts)
  if type(text) ~= "string" then
    return 0
  end
  local best, from = 0, 1
  while true do
    local at = string.find(text, "\n", from, true)
    best = math.max(best, width_of(font, string.sub(text, from, at and at - 1 or nil), opts))
    if not at then
      return best
    end
    from = at + 1
  end
end

local arrange, main_content, column_content_w

-- the size a child's main or cross axis contributes to its container's content size: `c` is the child, `axis` the
-- direction, `other` its resolved size the other way (a width when `axis` is "h") or nil
local function content_on(c, axis, other)
  if axis == "w" then
    return content_w(c)
  end
  return content_h(c, other)
end

-- the content width of a node, the same whatever its height
content_w = function(n)
  if n.cw then
    return n.cw
  end
  local k, v = n.kind, 0
  if k == "label" then
    v = widest_line(n.font, n.text, n.mopts)
  elseif k == "button" then
    if n.image and n.text == "" then
      v = math.tointeger(n.image.width)
    else
      v = width_of(n.font, n.text, n.mopts) + 24
    end
  elseif k == "switch" then
    local note = n.note and width_of(host.fonts.small, n.note) or 0
    v = 8 + math.max(width_of(n.font, n.text, n.mopts), note) + 12 + 64 + 8
  elseif k == "tabs" then
    local cells, wide = #n.items, 0
    for _, item in ipairs(n.items) do
      wide = math.max(wide, width_of(n.font, item, n.mopts))
    end
    v = cells * (wide + 24) + n.item_gap * (cells - 1)
  elseif k == "image" then
    v = math.tointeger(n.source.width)
  elseif k == "row" then
    v = main_content(n, nil)
  elseif k == "column" then
    v = column_content_w(n)
  elseif k == "grid" then
    v = grid_content_w(n)
  elseif k == "stack" then
    v = stack_content_w(n)
  end
  if n.reserve and (k == "label" or k == "button") then
    local extra = k == "button" and 24 or 0
    v = math.max(v, width_of(n.font, n.reserve, n.mopts) + extra)
  end
  -- a skin's left and right pad widen a button, and each cell of tabs
  if n.skin and (k == "button" or k == "tabs") then
    local _, r, _, l = skin_pad(n.skin)
    v = v + (k == "tabs" and #n.items or 1) * (l + r)
  end
  n.cw = v
  return v
end

-- the content height of a node whose width is `w`
content_h = function(n, w)
  n.ch = n.ch or {}
  local v = n.ch[w]
  if v then
    return v
  end
  local k, lh = n.kind, n.font and n.font.line_height
  v = 0
  if k == "label" then
    if w >= 1 and type(n.text) == "string" and n.text ~= "" then
      local _, height = n.font:layout(n.text, w, 32767, n.mopts)
      v = math.tointeger(height) or 0
    end
  elseif k == "button" and n.image and n.text == "" then
    v = math.tointeger(n.image.height)
  elseif k == "button" or k == "tabs" then
    v = lh + 16
    if n.skin then
      local t, _, b = skin_pad(n.skin)
      v = v + t + b
    end
  elseif k == "switch" then
    local line = lh
    if n.note then
      line = 2 * math.max(lh, host.fonts.small.line_height) + 4
    end
    v = math.max(34, line) + 16
  elseif k == "image" then
    v = math.tointeger(n.source.height)
  elseif k == "row" then
    local _, cross = arrange(n, math.max(0, w - pad(n, "w")), nil)
    v = cross + pad(n, "h")
  elseif k == "column" then
    v = main_content(n, math.max(0, w - pad(n, "w")))
  elseif k == "grid" then
    v = grid_content_h(n, w)
  elseif k == "stack" then
    v = stack_content_h(n, w)
  end
  n.ch[w] = v
  return v
end

local function align_of(n, c)
  local a = c.align_self or n.align_items
  if a == "baseline" and n.kind == "column" then
    return "start"
  end
  return a
end

-- the length of size `s` (an integer or `{ pct }`) against reference length `ref`, nil when it cannot be known
function length(s, ref)
  if type(s) == "number" then
    return s
  end
  if type(s) == "table" and ref then
    return ref * s.pct // 100
  end
end

-- The cross size of child `c` of container `n`, before stretching, and whether stretching may change it. `L` is
-- the inner cross length (nil when unknown), `M` the inner main length, `m` the child's main size (nil in a column,
-- where the cross axis comes first).
local function cross_of(n, c, L, M, m)
  local main, cross = MAIN[n.kind], CROSS[n.kind]
  local set = c[cross]
  local v = length(set, L)
  if v then
    v = math.max(v, pad(c, cross))
  elseif set == FIT then
    v = content_on(c, cross, m)
  elseif c.aspect and cross == "h" and m then
    v = round(m / c.aspect)
  elseif c.aspect and cross == "w" and length(c[main], M) then
    v = round(length(c[main], M) * c.aspect)
  else
    v = c.can_fit and content_on(c, cross, m) or 0
    return bound(c, cross, v), not c.can_fit or align_of(n, c) == "stretch"
  end
  return bound(c, cross, v), false
end

-- the width of child `c` of column `n` whose inner width is `L` (nil when unknown): stretched at once when the
-- column does not wrap and its width is known
local function column_child_w(n, c, L, M)
  local w, stretch = cross_of(n, c, L, M, nil)
  if stretch and L and not n.wrap then
    w = bound(c, "w", L)
  end
  return w, stretch
end

-- The content length of flex container `n` on its main axis. `iw` is its inner width when the main axis is the
-- height (a column's children are as wide as they are laid out); nil for a row.
main_content = function(n, iw)
  local main = MAIN[n.kind]
  local sum, count, grow, unit = 0, 0, 0, 0
  for _, c in ipairs(n.children) do
    if c then
      count = count + 1
      local other = main == "h" and column_child_w(n, c, iw, nil) or nil
      local set, v = c[main], nil
      if type(set) == "number" then
        v = math.max(set, pad(c, main))
      elseif set == FIT then
        v = content_on(c, main, other)
      elseif c.aspect and main == "w" and type(c.h) == "number" then
        v = round(c.h * c.aspect)
      elseif c.aspect and main == "h" then
        v = round(other / c.aspect)
      elseif c.grow > 0 then
        local fit = bound(c, main, c.can_fit and content_on(c, main, other) or 0)
        grow, unit = grow + c.grow, math.max(unit, ceil_div(fit, c.grow))
      else
        v = c.can_fit and content_on(c, main, other) or 0
      end
      if v then
        sum = sum + bound(c, main, v)
      end
    end
  end
  local gap = main == "w" and n.column_gap or n.row_gap
  return pad(n, main) + sum + grow * unit + gap * math.max(0, count - 1)
end

-- the content width of a column: its widest child, every child as if in one column
column_content_w = function(n)
  local widest = 0
  for _, c in ipairs(n.children) do
    if c then
      local v
      if type(c.w) == "number" then
        v = math.max(c.w, pad(c, "w"))
      elseif c.w == FIT then
        v = content_w(c)
      elseif c.aspect and type(c.h) == "number" then
        v = round(c.h * c.aspect)
      else
        v = c.can_fit and content_w(c) or 0
      end
      widest = math.max(widest, bound(c, "w", v))
    end
  end
  return pad(n, "w") + widest
end

-- the base of child `c` on the main axis: `M`, `L` are the inner lengths, `cw` the resolved width in a column
local function base_of(n, c, M, L, cw)
  local main = MAIN[n.kind]
  local set = c[main]
  local v = length(set, M)
  if v == nil then
    if set == FIT then
      v = content_on(c, main, cw)
    elseif c.aspect and main == "w" and length(c.h, L) then
      v = round(length(c.h, L) * c.aspect)
    elseif c.aspect and main == "h" then
      v = round(cw / c.aspect)
    elseif c.grow > 0 then
      v = 0
    elseif c.can_fit then
      v = content_on(c, main, cw)
    else
      v = 0
    end
  end
  return math.max(v, pad(c, main))
end

-- The freeze loop of one line (CSS Flexbox 9.7 with cumulative rounding): sets `it.size` of every item.
local function distribute(line, avail)
  local sum = 0
  for _, it in ipairs(line) do
    sum = sum + it.hyp
  end
  local growing = sum < avail
  for _, it in ipairs(line) do
    if growing then
      it.frozen = it.grow == 0 or it.base > it.hyp
    else
      it.frozen = it.shrink == 0 or it.base < it.hyp
    end
    it.size = it.hyp
  end
  while true do
    local open, used, total = {}, 0, 0
    for _, it in ipairs(line) do
      if it.frozen then
        used = used + it.size
      else
        open[#open + 1] = it
        used = used + it.base
        total = total + (growing and it.grow or it.shrink * it.inner)
      end
    end
    if #open == 0 then
      return
    end
    local free = avail - used
    local acc, prev, moved = 0, 0, 0
    for _, it in ipairs(open) do
      local target = it.base
      if total > 0 then
        acc = acc + (growing and it.grow or it.shrink * it.inner)
        if growing then
          target = it.base + free * acc // total - free * prev // total
        else
          local need = math.abs(free)
          target = math.max(0, it.base - (ceil_div(need * acc, total) - ceil_div(need * prev, total)))
        end
        prev = acc
      end
      it.target, it.clamped = target, clamp(target, it.lo, it.hi)
      moved = moved + it.clamped - target
    end
    for _, it in ipairs(open) do
      if moved == 0 or (moved > 0 and it.clamped > it.target) or (moved < 0 and it.clamped < it.target) then
        it.frozen, it.size = true, it.clamped
      end
    end
  end
end

-- the n + 1 slots `free` is split into by `mode` (a justify_content value), from slot 0 before the first item
function slots(mode, free, n)
  if free < 0 and SPACE[mode] then
    mode = "start"
  end
  local weights, total = {}, 0
  for i = 0, n do
    local edge = i == 0 or i == n
    local w
    if mode == "start" then
      w = i == n and 1 or 0
    elseif mode == "end" then
      w = i == 0 and 1 or 0
    elseif mode == "center" then
      w = edge and 1 or 0
    elseif mode == "space-between" then
      w = edge and 0 or 1
    elseif mode == "space-around" then
      w = edge and 1 or 2
    else
      w = 1
    end
    weights[i], total = w, total + w
  end
  if total == 0 then
    return slots("start", free, n)
  end
  local out, acc = {}, 0
  for i = 0, n do
    out[i] = free * (acc + weights[i]) // total - free * acc // total
    acc = acc + weights[i]
  end
  return out
end

local baseline

-- Lays out the children of flex container `n` in its inner box, main length `M` (known) and cross length `L` (nil
-- when unknown). Returns the items `{ node, index, main, cross, size, cross_size }` relative to the inner box, and
-- the total cross length of the lines.
arrange = function(n, M, L)
  local main, cross = MAIN[n.kind], CROSS[n.kind]
  local gap = main == "w" and n.column_gap or n.row_gap
  local cross_gap = main == "w" and n.row_gap or n.column_gap
  local items = {}
  for i, c in ipairs(n.children) do
    if c then
      local it = { node = c, index = i, grow = c.grow, shrink = c.shrink }
      if main == "h" then
        it.cross_size, it.stretch = column_child_w(n, c, L, M)
      end
      it.base = base_of(n, c, M, L, it.cross_size)
      -- not `and/or`: an unset bound is nil and would fall through to the other axis
      if main == "w" then
        it.lo, it.hi = c.lo_w, c.hi_w
      else
        it.lo, it.hi = c.lo_h, c.hi_h
      end
      it.hyp = clamp(it.base, it.lo, it.hi)
      it.inner = it.base - pad(c, main)
      items[#items + 1] = it
    end
  end
  -- lines, in declaration order
  local lines, line, sum = {}, {}, 0
  for _, it in ipairs(items) do
    if n.wrap and #line > 0 and sum + gap * #line + it.hyp > M then
      lines[#lines + 1], line, sum = line, {}, 0
    end
    line[#line + 1], sum = it, sum + it.hyp
  end
  if #line > 0 then
    lines[#lines + 1] = line
  end
  -- the main axis of each line, then the cross sizes and the line's cross length
  for _, ln in ipairs(lines) do
    local avail = M - gap * (#ln - 1)
    distribute(ln, avail)
    local used = 0
    for _, it in ipairs(ln) do
      used = used + it.size
    end
    local off = slots(n.justify, avail - used, #ln)
    local at = off[0]
    for k, it in ipairs(ln) do
      it.main, at = at, at + it.size + gap + off[k]
      if main == "w" then
        it.cross_size, it.stretch = cross_of(n, it.node, L, M, it.size)
      end
      it.align = it.stretch and "stretch" or align_of(n, it.node)
    end
    local longest, above, below = 0, 0, 0
    for _, it in ipairs(ln) do
      if it.align == "baseline" then
        local w = main == "w" and it.size or it.cross_size
        local h = main == "w" and it.cross_size or it.size
        it.baseline = baseline(it.node, w, h)
        above, below = math.max(above, it.baseline), math.max(below, it.cross_size - it.baseline)
      else
        longest = math.max(longest, it.cross_size)
      end
    end
    ln.above = above
    ln.length = math.max(longest, above + below)
    if L and not n.wrap then
      ln.length = L
    end
  end
  -- place the lines across
  local total = 0
  for _, ln in ipairs(lines) do
    total = total + ln.length
  end
  total = total + cross_gap * math.max(0, #lines - 1)
  if n.wrap and #lines > 0 then
    local free = L and (L - total) or 0
    local at = 0
    if n.align_content == "stretch" then
      for i, ln in ipairs(lines) do
        if free > 0 then
          ln.length = ln.length + free * i // #lines - free * (i - 1) // #lines
        end
        ln.start, at = at, at + ln.length + cross_gap
      end
    else
      local off = slots(n.align_content, free, #lines)
      at = off[0]
      for i, ln in ipairs(lines) do
        ln.start, at = at, at + ln.length + cross_gap + off[i]
      end
    end
  elseif #lines > 0 then
    lines[1].start = 0
  end
  -- each child within its line
  for _, ln in ipairs(lines) do
    for _, it in ipairs(ln) do
      local a = it.align
      if a == "stretch" and it.stretch then
        it.cross_size = bound(it.node, cross, ln.length)
      end
      local off = 0
      if a == "center" then
        off = (ln.length - it.cross_size) // 2
      elseif a == "end" then
        off = ln.length - it.cross_size
      elseif a == "baseline" then
        off = ln.above - it.baseline
      end
      it.cross = ln.start + off
    end
  end
  return items, total
end

-- The boxes of the children of container `n` whose box is `w` × `h`, relative to its top-left corner:
-- `{ node, index, x, y, w, h }` in declaration order, and its inner width and height.
local function boxes(n, w, h)
  local iw, ih = math.max(0, w - pad(n, "w")), math.max(0, h - pad(n, "h"))
  if n.kind == "grid" then
    return grid_boxes(n, iw, ih), iw, ih
  elseif n.kind == "stack" then
    return stack_boxes(n, iw, ih), iw, ih
  end
  local row = n.kind == "row"
  local items = arrange(n, row and iw or ih, row and ih or iw)
  local out = {}
  for k, it in ipairs(items) do
    local b = { node = it.node, index = it.index }
    if row then
      b.x, b.y, b.w, b.h = n.pad_l + it.main, n.pad_t + it.cross, it.size, it.cross_size
    else
      b.x, b.y, b.w, b.h = n.pad_l + it.cross, n.pad_t + it.main, it.cross_size, it.size
    end
    out[k] = b
  end
  return out, iw, ih
end

-- how far below the top of child `c` (`w` × `h`) its first text line sits, as the ascent places it
baseline = function(c, w, h)
  local k = c.kind
  if k == "label" then
    local block, off = content_h(c, w), 0
    if c.valign == "middle" then
      off = (h - block) // 2
    elseif c.valign == "bottom" then
      off = h - block
    end
    return off + c.font.ascent
  elseif k == "button" and c.image and c.text == "" then
    return h
  elseif k == "button" or k == "tabs" then
    -- centred in the text area that the skin's pad leaves, as skin_face gives it
    if c.skin then
      local t, _, b = skin_pad(c.skin)
      return t + (math.max(1, h - t - b) - c.font.line_height) // 2 + c.font.ascent
    end
    return (h - c.font.line_height) // 2 + c.font.ascent
  elseif k == "switch" then
    if c.note then
      return h // 2 - 2 - c.font.line_height + c.font.ascent
    end
    return (h - c.font.line_height) // 2 + c.font.ascent
  elseif CONTAINERS[k] then
    for _, b in ipairs((boxes(c, w, h))) do
      if b.node.kind ~= "spacer" then
        return b.y + baseline(b.node, b.w, b.h)
      end
    end
  end
  return h
end

-- Places node `n` at `x, y` with size `w` × `h`: sets `n.x n.y n.w n.h`, and for a container its children's too,
-- with `n.overflow` set to the child (counting skipped ones) that first leaves the inner box.
function place(n, x, y, w, h)
  n.x, n.y, n.w, n.h = x, y, w, h
  if not CONTAINERS[n.kind] then
    return
  end
  local list, iw, ih = boxes(n, w, h)
  for _, b in ipairs(list) do
    if n.overflow == nil and (b.x < n.pad_l or b.y < n.pad_t or b.x + b.w > n.pad_l + iw
      or b.y + b.h > n.pad_t + ih) then
      n.overflow = b.index
    end
    place(b.node, x + b.x, y + b.y, b.w, b.h)
  end
end

-- the size of the outermost container `n` from the call's `w` and `h` (an integer, or FIT for its content size)
function root_size(n, w, h)
  if w == FIT then
    w = content_w(n)
  end
  if h == FIT then
    h = content_h(n, w)
  end
  return w, h
end
