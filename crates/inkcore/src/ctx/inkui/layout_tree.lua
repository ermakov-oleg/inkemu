-- ctx.ui (§16, 2.15): the fields of containers and their children, checked into a tree of nodes for
-- layout_flex.lua and layout_grid.lua. After the children are counted, one walk finds the first invalid field or
-- exceeded limit among the structure and the sizing fields. In a frame, a control's other fields are checked when it
-- is registered.
--
-- A node: `kind` (nil for a plain box), `o` (the script's table), `id`, sizes `w`/`h` (nil, an integer, FIT or
-- `{ pct = N }`), bounds `lo_w hi_w lo_h hi_h` (nil `hi` is unlimited), `grow`, `shrink`, `align_self`,
-- `justify_self`, `column_span`, `row_span`, `aspect`, `can_fit`, `font`, `hidden`, and for a container
-- `pad_t pad_r pad_b pad_l`, the gaps, alignments, `wrap`, a grid's tracks (`columns` or `min_column`, `rows`,
-- `auto_row`), the appearance, `children` (a node, or false where the child is skipped) and `nodes` (the same with
-- hidden ones). A track is `{ px = N }`, `{ fit = true }`, `{ fr = N }` or `{ pct = N }`.

FIT = "fit"
local MAX_LEVELS = 16
local MAX_CHILDREN = 1024
local MAX_TRACKS = 64

-- every container kind, and the flex ones among them
CONTAINERS = { row = true, column = true, grid = true, stack = true }
FLEX = { row = true, column = true }
-- the kinds of control a container child can be
CONTROLS = {
  label = true, button = true, switch = true, image = true, field = true, region = true,
  tabs = true, radio = true, stepper = true, pager = true, list = true, progress = true,
}
local CAN_FIT = {
  label = true, button = true, switch = true, tabs = true, image = true, spacer = true,
  row = true, column = true, grid = true, stack = true,
}
-- text controls that keep a line's height unless `h` is a number
local ONE_LINE = { label = true, button = true, switch = true, tabs = true }

local JUSTIFY = {
  start = true, center = true, ["end"] = true,
  ["space-between"] = true, ["space-around"] = true, ["space-evenly"] = true,
}
local ALIGN = { start = true, center = true, ["end"] = true, stretch = true, baseline = true }
-- a grid or stack places in a cell, where no baseline is shared
local CELL_ALIGN = { start = true, center = true, ["end"] = true, stretch = true }
local ALIGN_CONTENT = { stretch = true }
for k in pairs(JUSTIFY) do ALIGN_CONTENT[k] = true end

-- the container fields a kind does not take, and the fields a child of that kind of container does not take:
-- setting one at all, even to its default, is invalid
local FLEX_ONLY = { "columns", "rows", "auto_rows", "justify_items" }
local FLEX_ONLY_CHILD = { "justify_self", "column_span", "row_span" }
local NOT_FOR = {
  row = FLEX_ONLY,
  column = FLEX_ONLY,
  grid = { "wrap", "divider" },
  stack = {
    "gap", "row_gap", "column_gap", "justify_content", "align_content", "wrap", "divider",
    "columns", "rows", "auto_rows",
  },
}
local NOT_FOR_CHILD = {
  row = FLEX_ONLY_CHILD,
  column = FLEX_ONLY_CHILD,
  grid = { "grow", "shrink" },
  stack = { "grow", "shrink", "column_span", "row_span" },
}
-- keys of a ui:layout result that an id cannot take
local BOX_KEYS = { x = true, y = true, w = true, h = true, overflow = true }

local function int_or(v, default)
  if v == nil then
    return default
  end
  return math.tointeger(v)
end

-- nil, a whole number >= 1, FIT, `{ pct = N }` for "N%" (N 1..100), or false when invalid
local function size_of(v)
  if v == nil or v == FIT then
    return v
  end
  if is_side(v) then
    return math.tointeger(v)
  end
  if type(v) == "string" then
    local digits = string.match(v, "^(%d+)%%$")
    local n = digits and math.tointeger(tonumber(digits))
    if n and n >= 1 and n <= 100 then
      return { pct = n }
    end
  end
  return false
end

local function fail(cx, code)
  cx.err = cx.err or code
end

-- A skin field: an unknown name is invalid; a registered one becomes the checked skin, which a container draws and
-- a button's or tabs' content size and baseline take their pad from.
local function tree_skin(cx, n, skin)
  local ok, checked = check_skin(cx.st, skin)
  if not ok then
    return fail(cx, INVALID)
  end
  n.skin = checked
end

-- a grid track, or nil when invalid
local function track_of(v)
  if is_side(v) then
    return { px = math.tointeger(v) }
  end
  if v == FIT then
    return { fit = true }
  end
  if type(v) == "string" then
    local digits = string.match(v, "^(%d+)fr$")
    local fr = digits and math.tointeger(tonumber(digits))
    if fr and fr >= 1 and fr <= 1000 then
      return { fr = fr }
    end
    local pct = size_of(v)
    if pct then
      return pct
    end
  end
end

-- an array of 1..MAX_TRACKS tracks, or nil when invalid
local function tracks_of(list)
  if type(list) ~= "table" or #list < 1 or #list > MAX_TRACKS then
    return nil
  end
  local out = {}
  for i = 1, #list do
    out[i] = track_of(list[i])
    if not out[i] then
      return nil
    end
  end
  return out
end

-- the tracks and item alignment of a grid, or the item alignment of a stack
local function check_cells(cx, n, o)
  if not (o.justify_items == nil or CELL_ALIGN[o.justify_items])
    or not (o.align_items == nil or CELL_ALIGN[o.align_items]) then
    return fail(cx, INVALID)
  end
  n.justify_items = o.justify_items or "stretch"
  if n.kind == "stack" then
    return
  end
  local c = o.columns
  if is_integral(c) then
    if c < 1 or c > MAX_TRACKS then
      return fail(cx, INVALID)
    end
    n.columns = {}
    for i = 1, c do
      n.columns[i] = { fr = 1 }
    end
  elseif type(c) == "table" and c.min ~= nil then
    -- the count of `{ min }` columns waits for the inner width
    if not is_side(c.min) or #c > 0 then
      return fail(cx, INVALID)
    end
    n.min_column = math.tointeger(c.min)
  else
    n.columns = tracks_of(c)
    if not n.columns then
      return fail(cx, INVALID)
    end
  end
  if o.rows ~= nil then
    n.rows = tracks_of(o.rows)
    if not n.rows then
      return fail(cx, INVALID)
    end
  end
  n.auto_row = o.auto_rows == nil and { fit = true } or track_of(o.auto_rows)
  if not n.auto_row then
    return fail(cx, INVALID)
  end
end

-- padding, gaps, alignment, wrap, divider, a grid's tracks and the appearance of a container
local function check_container(cx, n, o)
  for _, k in ipairs(NOT_FOR[n.kind]) do
    if o[k] ~= nil then
      return fail(cx, INVALID)
    end
  end
  local p = o.padding
  if p == nil then
    p = { 0 }
  elseif int_in(p, 0, 255) then
    p = { p }
  elseif type(p) ~= "table" or #p < 1 or #p > 4 then
    return fail(cx, INVALID)
  end
  for i = 1, #p do
    if not int_in(p[i], 0, 255) then
      return fail(cx, INVALID)
    end
  end
  local t, r, b, l = p[1], p[2] or p[1], p[3] or p[1], p[4] or p[2] or p[1]
  n.pad_t, n.pad_r, n.pad_b, n.pad_l = math.tointeger(t), math.tointeger(r), math.tointeger(b), math.tointeger(l)
  for _, k in ipairs({ "gap", "row_gap", "column_gap" }) do
    if o[k] ~= nil and not int_in(o[k], 0, 255) then
      return fail(cx, INVALID)
    end
  end
  local gap = int_or(o.gap, 0)
  n.row_gap, n.column_gap = int_or(o.row_gap, gap), int_or(o.column_gap, gap)
  local flex = FLEX[n.kind]
  if not (o.justify_content == nil or JUSTIFY[o.justify_content])
    or not (o.align_items == nil or ALIGN[o.align_items])
    or not (o.align_content == nil or (flex and ALIGN_CONTENT or JUSTIFY)[o.align_content])
    or not optional(o.wrap, "boolean") then
    return fail(cx, INVALID)
  end
  n.justify = o.justify_content or "start"
  n.align_items = o.align_items or "stretch"
  n.align_content = o.align_content or (flex and "stretch" or "start")
  n.wrap = o.wrap == true
  if not flex then
    check_cells(cx, n, o)
    if cx.err then
      return
    end
  end
  local d = o.divider
  if d == true then
    d = {}
  end
  if d ~= nil and d ~= false then
    if type(d) ~= "table" or n.wrap
      or not (d.line_width == nil or int_in(d.line_width, 1, 255))
      or not (d.color == nil or int_in(d.color, 0, 15))
      or not (d.inset == nil or int_in(d.inset, 0, 255)) then
      return fail(cx, INVALID)
    end
    n.divider = { t = int_or(d.line_width, 1), color = int_or(d.color, 15), inset = int_or(d.inset, 0) }
  end
  if not optional(o.border, "boolean")
    or not (o.line_width == nil or int_in(o.line_width, 1, 255))
    or not (o.border_color == nil or int_in(o.border_color, 0, 15))
    or not (o.background == nil or int_in(o.background, 0, 15))
    or not (o.radius == nil or int_in(o.radius, 0, 255)) then
    return fail(cx, INVALID)
  end
  n.border = o.border == true
  n.line_width, n.border_color = int_or(o.line_width, 1), int_or(o.border_color, 15)
  n.background, n.radius = int_or(o.background, nil), int_or(o.radius, 0)
  tree_skin(cx, n, o.skin)
end

-- the fields a control's content size reads, checked as that control checks them
local function check_sizing(cx, n, o)
  local k = n.kind
  n.font = is_font(o.font) and o.font or host.fonts.medium
  if k == "label" or k == "button" or k == "switch" then
    -- any other text goes to text_box as it is, but cannot be measured: a label's height or a fitted width
    local measured = k == "label" and type(o.h) ~= "number" or k ~= "label" and o.w == FIT
    if not optional(o.text, "string") and measured then
      return fail(cx, INVALID)
    end
    n.text = o.text or ""
  end
  if k == "label" or k == "button" then
    if not check_text_level(n, o) then
      return fail(cx, INVALID)
    end
    if api_at_least(16) then
      n.reserve = o.reserve
    end
    n.valign = o.valign
    n.mopts = measure_opts(n)
  end
  if k == "button" then
    if o.image ~= nil and o.skin ~= nil then
      return fail(cx, INVALID)
    end
    if o.image ~= nil and not is_image(o.image) then
      return fail(cx, INVALID)
    end
    n.image = o.image
    tree_skin(cx, n, o.skin)
  elseif k == "switch" then
    if not (optional(o.note, "string") and (not api_at_least(16) or optional(o.bold, "boolean"))) then
      return fail(cx, INVALID)
    end
    n.note = o.note
    -- from 2.16 the text of a switch is bold unless the script says otherwise
    n.mopts = api_at_least(16) and { bold = o.bold ~= false } or nil
    tree_skin(cx, n, o.skin)
  elseif k == "tabs" or k == "radio" then
    if not (check_items(o.items) and check_cell_gap(o.gap)) then
      return fail(cx, INVALID)
    end
    n.items, n.item_gap = o.items, int_or(o.gap, 0)
    if k == "tabs" then
      if api_at_least(16) and not optional(o.bold, "boolean") then
        return fail(cx, INVALID)
      end
      -- from 2.16 every cell is measured bold unless the script says otherwise
      n.mopts = api_at_least(16) and { bold = o.bold ~= false } or nil
    end
    tree_skin(cx, n, o.skin)
  elseif k == "image" then
    if not is_image(o.source) then
      return fail(cx, INVALID)
    end
    n.source = o.source
  end
end

-- justify_self of a grid or stack child, and its spans in a grid
local function check_cell_child(cx, n, o, parent)
  if not (o.justify_self == nil or CELL_ALIGN[o.justify_self]) then
    return fail(cx, INVALID)
  end
  n.justify_self = o.justify_self
  if parent.kind ~= "grid" then
    return
  end
  for _, k in ipairs({ "column_span", "row_span" }) do
    if o[k] ~= nil and not int_in(o[k], 1, MAX_TRACKS) then
      return fail(cx, INVALID)
    end
  end
  n.column_span, n.row_span = int_or(o.column_span, 1), int_or(o.row_span, 1)
  -- `{ min }` columns are counted only once the width is known; layout_grid.lua refuses a longer span then
  if parent.columns and n.column_span > #parent.columns then
    return fail(cx, INVALID)
  end
end

-- w, h, min/max, aspect_ratio, align_self, grow, shrink, justify_self, the spans and hidden of a child of `parent`
local function check_child(cx, n, o, parent)
  for _, k in ipairs(NOT_FOR_CHILD[parent.kind]) do
    if o[k] ~= nil then
      return fail(cx, INVALID)
    end
  end
  local w, h = size_of(o.w), size_of(o.h)
  if w == false or h == false or ((w == FIT or h == FIT) and not n.can_fit) then
    return fail(cx, INVALID)
  end
  -- a grid that counts its columns from its width cannot take its width from them
  if w == FIT and n.min_column then
    return fail(cx, INVALID)
  end
  n.w, n.h = w, h
  for _, k in ipairs({ "min_w", "max_w", "min_h", "max_h" }) do
    if o[k] ~= nil and not (is_integral(o[k]) and o[k] >= 0) then
      return fail(cx, INVALID)
    end
  end
  if (o.min_w and o.max_w and o.min_w > o.max_w) or (o.min_h and o.max_h and o.min_h > o.max_h) then
    return fail(cx, INVALID)
  end
  local a = o.aspect_ratio
  if a ~= nil and not (type(a) == "number" and a > 0 and a < math.huge) then
    return fail(cx, INVALID)
  end
  if a ~= nil and h == FIT and w == nil then
    return fail(cx, INVALID)
  end
  n.aspect = a
  local flex = FLEX[parent.kind]
  if not (o.align_self == nil or (flex and ALIGN or CELL_ALIGN)[o.align_self])
    or not (o.grow == nil or int_in(o.grow, 0, 1000))
    or not (o.shrink == nil or int_in(o.shrink, 0, 1000))
    or not optional(o.hidden, "boolean") then
    return fail(cx, INVALID)
  end
  n.align_self = o.align_self
  n.hidden = o.hidden == true
  if flex then
    local main = parent.kind == "row" and "w" or "h"
    n.grow = int_or(o.grow, (n.kind == "spacer" and n[main] == nil) and 1 or 0)
    n.shrink = int_or(o.shrink, 1)
    -- a box whose main size can come from nothing: no size, no grow, no content, no aspect ratio to derive it from
    local derivable = a ~= nil and (main == "h" or type(h) == "number" or type(h) == "table")
    if n[main] == nil and n.grow == 0 and not n.can_fit and not derivable then
      return fail(cx, INVALID)
    end
  else
    check_cell_child(cx, n, o, parent)
    if cx.err then
      return
    end
  end
  local spacer = n.kind == "spacer"
  local lo_h = o.min_h
  if lo_h == nil and flex and ONE_LINE[n.kind] and (h == nil or h == FIT) then
    lo_h = n.font.line_height
    if o.max_h ~= nil and o.max_h < lo_h then
      lo_h = o.max_h
    end
  end
  n.lo_w = int_or(o.min_w, spacer and 0 or 1)
  n.lo_h = int_or(lo_h, spacer and 0 or 1)
  n.hi_w, n.hi_h = int_or(o.max_w, nil), int_or(o.max_h, nil)
  -- a container child keeps at least its own padding
  if CONTAINERS[n.kind] then
    local pw, ph = n.pad_l + n.pad_r, n.pad_t + n.pad_b
    n.lo_w, n.lo_h = math.max(n.lo_w, pw), math.max(n.lo_h, ph)
    n.hi_w, n.hi_h = n.hi_w and math.max(n.hi_w, pw), n.hi_h and math.max(n.hi_h, ph)
  end
end

local node_of

-- the id of a node: a string never used before in this tree (nor in the open frame); required where `need`
local function check_id(cx, n, id, need)
  if id == nil then
    if need then
      fail(cx, INVALID)
    end
    return
  end
  if type(id) ~= "string" or id == "" or cx.ids[id] or (cx.frame and cx.frame.ids[id])
    or (not cx.frame and BOX_KEYS[id]) then
    return fail(cx, INVALID)
  end
  cx.ids[id] = true
  n.id = id
end

local function check_children(cx, n, o, level)
  local list = o.children
  if list == nil then
    list = {}
  elseif type(list) ~= "table" then
    return fail(cx, INVALID)
  end
  -- `children` is what the layout places; `nodes` also keeps the hidden ones, whose fields a frame still checks
  n.children, n.nodes = {}, {}
  for i = 1, #list do
    local item = list[i]
    if item == false then
      n.children[i], n.nodes[i] = false, false
    elseif type(item) ~= "table" then
      return fail(cx, INVALID)
    else
      local child = node_of(cx, item, n, level)
      if cx.err then
        return
      end
      n.children[i] = (child and not child.hidden) and child or false
      n.nodes[i] = child or false
    end
  end
end

-- the child tables of the tree under `o`, counted before anything else is checked; stops past MAX_CHILDREN
local function count_children(o, n)
  local list = o.children
  if type(list) ~= "table" then
    return n
  end
  for i = 1, #list do
    local item = list[i]
    if type(item) == "table" then
      n = n + 1
      if n > MAX_CHILDREN then
        return n
      end
      if CONTAINERS[item.kind] then
        n = count_children(item, n)
      end
    end
  end
  return n
end

-- the node of `o`, a child of the container `parent` at nesting `level` (the outermost container is level 1)
node_of = function(cx, o, parent, level)
  local kind = o.kind
  local n = { kind = kind, o = o, can_fit = CAN_FIT[kind] == true }
  if CONTAINERS[kind] then
    if level + 1 > MAX_LEVELS then
      return fail(cx, "limit_exceeded")
    end
    check_container(cx, n, o)
  elseif kind == nil then
    if cx.frame then
      return fail(cx, INVALID)
    end
  elseif kind ~= "spacer" and not CONTROLS[kind] then
    return fail(cx, INVALID)
  elseif kind ~= "spacer" and cx.frame and level + 1 > MAX_LEVELS then
    -- in a frame the control inside the innermost container counts as a level of its own
    return fail(cx, "limit_exceeded")
  end
  if cx.err then
    return
  end
  -- a hidden child is absent, so its id takes no key in the result and clashes with nothing
  if o.hidden ~= true then
    check_id(cx, n, o.id, cx.frame and CONTROLS[kind])
  end
  if cx.err then
    return
  end
  if kind ~= nil and kind ~= "spacer" then
    check_sizing(cx, n, o)
  end
  if cx.err then
    return
  end
  check_child(cx, n, o, parent)
  if cx.err then
    return
  end
  if CONTAINERS[kind] then
    check_children(cx, n, o, level + 1)
  end
  return n
end

-- The tree of the outermost container `o` of kind `kind`, `w` wide (an integer, or FIT). `frame` is the open frame
-- when the container is declared in one as `id` (nil for ui:layout); `st` is the UI object's state. Returns the root
-- node, or nil and the error code.
function layout_tree(st, frame, id, kind, o, w)
  local cx = { st = st, frame = frame, ids = {} }
  if id ~= nil then
    cx.ids[id] = true
  end
  if not CONTAINERS[kind] then
    return nil, INVALID
  end
  if count_children(o, 0) > MAX_CHILDREN then
    return nil, "limit_exceeded"
  end
  local n = { kind = kind, o = o, id = id, can_fit = true }
  check_container(cx, n, o)
  if not cx.err and w == FIT and n.min_column then
    fail(cx, INVALID)
  end
  if not cx.err then
    check_children(cx, n, o, 1)
  end
  if cx.err then
    return nil, cx.err
  end
  return n
end

-- Before 2.15 a container's children all have integer sizes and follow each other from its corner, `gap` apart;
-- the 2.15 fields are ignored. Sets `n.x n.y n.w n.h` of every node.
local function old_children(cx, n, o, level)
  local list = o.children
  if list == nil then
    list = {}
  end
  if type(list) ~= "table" or not (o.gap == nil or (is_integral(o.gap) and o.gap >= 0)) then
    return fail(cx, INVALID)
  end
  local gap, at = int_or(o.gap, 0), 0
  n.children = {}
  for i = 1, #list do
    local item = list[i]
    if type(item) ~= "table" or not (is_side(item.w) and is_side(item.h)) then
      return fail(cx, INVALID)
    end
    local kind = item.kind
    local c = { kind = kind, o = item, w = math.tointeger(item.w), h = math.tointeger(item.h) }
    if n.kind == "row" then
      c.x, c.y, at = n.x + at, n.y, at + c.w + gap
    else
      c.x, c.y, at = n.x, n.y + at, at + c.h + gap
    end
    if not (FLEX[kind] or CONTROLS[kind]) then
      return fail(cx, INVALID)
    end
    if level + 1 > MAX_LEVELS then
      return fail(cx, "limit_exceeded")
    end
    check_id(cx, c, item.id, CONTROLS[kind])
    if cx.err then
      return
    end
    if FLEX[kind] then
      old_children(cx, c, item, level + 1)
    end
    if cx.err then
      return
    end
    n.children[i] = c
  end
end

-- The tree of a row or column declared as `id` in `frame` by a package for API 2.14 or earlier, placed at `x, y`
-- with size `w` × `h`. Returns as layout_tree does.
function old_tree(st, frame, id, kind, o, x, y, w, h)
  local cx = { st = st, frame = frame, ids = { [id] = true } }
  local n = { kind = kind, o = o, id = id, x = x, y = y, w = w, h = h }
  old_children(cx, n, o, 1)
  if cx.err then
    return nil, cx.err
  end
  return n
end
