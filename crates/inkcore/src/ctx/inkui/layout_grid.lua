-- ctx.ui (§16, 2.15): the layout of `grid` and `stack` nodes: automatic placement, track sizing, a child's place
-- in its cell, and the content sizes. A stack is one cell over its whole inner box. Integers only; the splits use
-- cumulative rounding as flexbox does (layout_flex.lua holds the shared helpers).

-- The size of child `c` of grid or stack `n` across a cell region `len` wide, and its offset there. `href` is the
-- region's height when a percentage height may derive the width through `aspect_ratio` (a stack), else nil.
local function cell_w(n, c, len, href)
  local a = c.justify_self or n.justify_items
  local v, stretched = length(c.w, len), false
  if v then
    v = math.max(v, pad(c, "w"))
  elseif c.w == FIT then
    v = content_w(c)
  elseif c.aspect and length(c.h, href) then
    v = round(length(c.h, href) * c.aspect)
  elseif a == "stretch" or not c.can_fit then
    v, stretched = len, true
  else
    v = content_w(c)
  end
  v = bound(c, "w", v)
  if stretched or a == "start" or a == "stretch" then
    return v, 0
  end
  return v, a == "center" and (len - v) // 2 or len - v
end

-- the same down a region `len` high, for a child `fw` wide
local function cell_h(n, c, len, fw)
  local a = c.align_self or n.align_items
  local v, stretched = length(c.h, len), false
  if v then
    v = math.max(v, pad(c, "h"))
  elseif c.h == FIT then
    v = content_h(c, fw)
  elseif c.aspect then
    v = round(fw / c.aspect)
  elseif a == "stretch" or not c.can_fit then
    v, stretched = len, true
  else
    v = content_h(c, fw)
  end
  v = bound(c, "h", v)
  if stretched or a == "start" or a == "stretch" then
    return v, 0
  end
  return v, a == "center" and (len - v) // 2 or len - v
end

-- the width child `c` asks for: its integer width, one derived from an integer height, or its content width
local function want_w(c)
  local v
  if type(c.w) == "number" then
    v = math.max(c.w, pad(c, "w"))
  elseif c.w == nil and c.aspect and type(c.h) == "number" then
    v = round(c.h * c.aspect)
  else
    v = c.can_fit and content_w(c) or 0
  end
  return bound(c, "w", v)
end

-- the height child `c` asks for when it is `fw` wide
local function want_h(c, fw)
  local v
  if type(c.h) == "number" then
    v = math.max(c.h, pad(c, "h"))
  elseif c.h == nil and c.aspect then
    v = round(fw / c.aspect)
  else
    v = c.can_fit and content_h(c, fw) or 0
  end
  return bound(c, "h", v)
end

-- raised while a layout is computed when a span is longer than the `{ min }` columns its grid's width gives;
-- lay_out turns it into invalid_argument
SPAN_TOO_LONG = setmetatable({}, { __tostring = function() return "column_span past the columns" end })

-- Places the children of grid `n` over `count` columns in declaration order, each from just after the previous
-- one, in the first free cells that hold it; holes left behind stay empty. Returns the items
-- `{ node, index, col, row, cspan, rspan }` and the number of rows they reach.
local function place_cells(n, count)
  local items, taken, row, col, rows = {}, {}, 1, 1, 0
  local function free(r, c, cs, rs)
    for y = r, r + rs - 1 do
      local line = taken[y]
      if line then
        for x = c, c + cs - 1 do
          if line[x] then
            return false
          end
        end
      end
    end
    return true
  end
  for i, c in ipairs(n.children) do
    if c then
      local cs, rs = c.column_span, c.row_span
      if cs > count then
        error(SPAN_TOO_LONG, 0)
      end
      while true do
        if col + cs - 1 > count then
          row, col = row + 1, 1
        elseif free(row, col, cs, rs) then
          break
        else
          col = col + 1
        end
      end
      for y = row, row + rs - 1 do
        taken[y] = taken[y] or {}
        for x = col, col + cs - 1 do
          taken[y][x] = true
        end
      end
      items[#items + 1] = { node = c, index = i, col = col, row = row, cspan = cs, rspan = rs }
      rows = math.max(rows, row + rs - 1)
      col = col + cs
    end
  end
  return items, rows
end

-- The sizes of `tracks` along a direction `len` long (nil while a content size is computed), `gap` apart. Item
-- `it` starts at track `it[from]`, spans `it[span]` tracks and asks for `it[want]`.
local function size_tracks(tracks, len, gap, items, from, span, want)
  local count, size, kind = #tracks, {}, {}
  for i, t in ipairs(tracks) do
    if t.px then
      size[i], kind[i] = t.px, "px"
    elseif t.pct and len then
      size[i], kind[i] = len * t.pct // 100, "px"
    elseif t.fr then
      size[i], kind[i] = 0, "fr"
    else
      -- a percentage of a length not yet known sizes as "fit"
      size[i], kind[i] = 0, "fit"
    end
  end
  -- one-track items size their fit track; an fr track remembers its largest for an unknown length
  local alone, groups = {}, {}
  for _, it in ipairs(items) do
    local i, s = it[from], it[span]
    if s == 1 then
      if kind[i] == "fit" then
        size[i] = math.max(size[i], it[want])
      elseif kind[i] == "fr" then
        alone[i] = math.max(alone[i] or 0, it[want])
      end
    else
      groups[s] = groups[s] or {}
      table.insert(groups[s], it)
    end
  end
  -- spanning items by span, smallest first: what exceeds the crossed tracks goes evenly to their fit tracks, an
  -- item crossing an fr track gives nothing, and each track grows by the most a group gives it
  for s = 2, count do
    local add = {}
    for _, it in ipairs(groups[s] or {}) do
      local first, crossed, fits, fr = it[from], gap * (s - 1), {}, false
      for i = first, first + s - 1 do
        crossed = crossed + size[i]
        fr = fr or kind[i] == "fr"
        if kind[i] == "fit" then
          fits[#fits + 1] = i
        end
      end
      local extra = it[want] - crossed
      if not fr and extra > 0 and #fits > 0 then
        for k, i in ipairs(fits) do
          add[i] = math.max(add[i] or 0, extra * k // #fits - extra * (k - 1) // #fits)
        end
      end
    end
    for i, v in pairs(add) do
      size[i] = size[i] + v
    end
  end
  -- the fr tracks: the leftover by ratio, or as many units as the largest content needs
  if len then
    local free, total = len - gap * math.max(0, count - 1), 0
    for i, t in ipairs(tracks) do
      if kind[i] == "fr" then
        total = total + t.fr
      else
        free = free - size[i]
      end
    end
    if free > 0 and total > 0 then
      local acc = 0
      for i, t in ipairs(tracks) do
        if kind[i] == "fr" then
          size[i] = free * (acc + t.fr) // total - free * acc // total
          acc = acc + t.fr
        end
      end
    end
  else
    local unit = 0
    for i, t in ipairs(tracks) do
      if kind[i] == "fr" and alone[i] then
        unit = math.max(unit, ceil_div(alone[i], t.fr))
      end
    end
    for i, t in ipairs(tracks) do
      if kind[i] == "fr" then
        size[i] = t.fr * unit
      end
    end
  end
  return size
end

-- the sum of `size` with `gap` between the tracks
local function span_of(size, gap)
  local total = gap * math.max(0, #size - 1)
  for _, v in ipairs(size) do
    total = total + v
  end
  return total
end

-- Where each track starts when the leftover of a direction `len` long (nil: none) goes into the gaps by `mode`
-- (a justify_content value).
local function starts(size, gap, len, mode)
  local off = slots(mode, len and (len - span_of(size, gap)) or 0, #size)
  local out, at = {}, off[0]
  for i, v in ipairs(size) do
    out[i] = at
    at = at + v + gap + off[i]
  end
  return out
end

-- the start and length of the region from track `first` over `count` tracks
local function region(start, size, first, count)
  local last = first + count - 1
  return start[first], start[last] + size[last] - start[first]
end

-- The columns of grid `n` in an inner width `iw` (nil while its content width is computed): the placed items with
-- their wanted widths, the column sizes and the number of rows.
local function columns(n, iw)
  local tracks = n.columns
  if tracks == nil then
    local g = n.column_gap
    tracks = {}
    for i = 1, math.max(1, (iw + g) // (n.min_column + g)) do
      tracks[i] = { fr = 1 }
    end
  end
  local items, rows = place_cells(n, #tracks)
  for _, it in ipairs(items) do
    it.want_w = want_w(it.node)
  end
  return items, size_tracks(tracks, iw, n.column_gap, items, "col", "cspan", "want_w"), rows
end

-- Lays out grid `n` across an inner width `iw` and down an inner height `ih` (nil while its content height is
-- computed): sets `x w` and, with `ih`, `y h` of every item. Returns the items and the row sizes.
local function arrange_grid(n, iw, ih)
  local items, cols, count = columns(n, iw)
  local x0 = starts(cols, n.column_gap, iw, n.justify)
  for _, it in ipairs(items) do
    local at, len = region(x0, cols, it.col, it.cspan)
    local w, off = cell_w(n, it.node, len, nil)
    it.x, it.w = at + off, w
    it.want_h = want_h(it.node, w)
  end
  local tracks = {}
  for r = 1, math.max(count, n.rows and #n.rows or 0) do
    tracks[r] = n.rows and n.rows[r] or n.auto_row
  end
  local rows = size_tracks(tracks, ih, n.row_gap, items, "row", "rspan", "want_h")
  if ih then
    local y0 = starts(rows, n.row_gap, ih, n.align_content)
    for _, it in ipairs(items) do
      local at, len = region(y0, rows, it.row, it.rspan)
      local h, off = cell_h(n, it.node, len, it.w)
      it.y, it.h = at + off, h
    end
  end
  return items, rows
end

function grid_content_w(n)
  -- `{ min }` columns need the width they are counted from: such a grid asks for none
  if n.min_column then
    return 0
  end
  local _, cols = columns(n, nil)
  return pad(n, "w") + span_of(cols, n.column_gap)
end

function grid_content_h(n, w)
  local _, rows = arrange_grid(n, math.max(0, w - pad(n, "w")), nil)
  return pad(n, "h") + span_of(rows, n.row_gap)
end

-- the boxes of the children of grid `n` in its inner box `iw` × `ih`, relative to its corner, in declaration order
function grid_boxes(n, iw, ih)
  local out = {}
  for k, it in ipairs((arrange_grid(n, iw, ih))) do
    out[k] = { node = it.node, index = it.index, x = n.pad_l + it.x, y = n.pad_t + it.y, w = it.w, h = it.h }
  end
  return out
end

function stack_content_w(n)
  local most = 0
  for _, c in ipairs(n.children) do
    if c then
      most = math.max(most, want_w(c))
    end
  end
  return pad(n, "w") + most
end

function stack_content_h(n, w)
  local iw, most = math.max(0, w - pad(n, "w")), 0
  for _, c in ipairs(n.children) do
    if c then
      most = math.max(most, want_h(c, (cell_w(n, c, iw, nil))))
    end
  end
  return pad(n, "h") + most
end

-- the boxes of the children of stack `n`, every one placed in the whole inner box `iw` × `ih`
function stack_boxes(n, iw, ih)
  local out = {}
  for i, c in ipairs(n.children) do
    if c then
      local w, x = cell_w(n, c, iw, ih)
      local h, y = cell_h(n, c, ih, w)
      out[#out + 1] = { node = c, index = i, x = n.pad_l + x, y = n.pad_t + y, w = w, h = h }
    end
  end
  return out
end
