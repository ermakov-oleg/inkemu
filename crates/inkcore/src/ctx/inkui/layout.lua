-- ctx.ui (§16): `row`, `column`, `grid` and `stack` in a frame, `ui:layout`, and how a container is drawn. A
-- package for API 2.15 or later gets the 2.15 layout (layout_tree.lua, layout_flex.lua, layout_grid.lua); an older
-- one the earlier rules for a row or column, where every child is sized and they follow each other, and no grid or
-- stack.

-- An integer from the corner of a fill row `dy` of a rounded box of radius `r`: round(sqrt(r² − (r − dy − 0.5)²)),
-- computed on integers as the k with (2k − 1)² <= 4r² − (2r − 2dy − 1)² < (2k + 1)².
local function corner(r, dy)
  local b = 2 * r - 2 * dy - 1
  local a = 4 * r * r - b * b
  local s = math.tointeger(math.floor(math.sqrt(a)))
  while s * s > a do
    s = s - 1
  end
  while (s + 1) * (s + 1) <= a do
    s = s + 1
  end
  return (s + 1) // 2
end

-- A colour fill: from 2.16, on a canvas with `pattern`, in four bands (paper, light dots, dark dots, ink), a
-- rounded halftone one dot row at a time in the corners. `radius` is nil for a divider.
local function band(g, x, y, w, h, color, radius)
  if not api_at_least(16) or g.pattern == nil then
    g:rect(x, y, w, h, { color = color, fill = true, radius = radius })
    return
  end
  if color <= 2 or color >= 13 then
    g:rect(x, y, w, h, { color = color <= 2 and 0 or 15, fill = true, radius = radius })
    return
  end
  g:rect(x, y, w, h, { color = 0, fill = true, radius = radius })
  local halftone = { color = 15, density = color <= 7 and 38 or 69, kind = "dot" }
  local r = math.min(radius or 0, w // 2, h // 2)
  for dy = 0, r - 1 do
    local inset = r - corner(r, dy)
    g:pattern(x + inset, y + dy, w - 2 * inset, 1, halftone)
    g:pattern(x + inset, y + h - 1 - dy, w - 2 * inset, 1, halftone)
  end
  if h - 2 * r > 0 then
    g:pattern(x, y + r, w, h - 2 * r, halftone)
  end
end

-- the background and the border (or the skin's normal look instead), then the dividers of container node `n`,
-- under its children
local function paint_container(g, c)
  local n = c.node
  if n.skin then
    skin_face(g, n.skin, n.x, n.y, n.w, n.h, {})
  else
    if n.background then
      band(g, n.x, n.y, n.w, n.h, n.background, n.radius)
    end
    if n.border then
      g:rect(n.x, n.y, n.w, n.h, { color = n.border_color, line_width = n.line_width, radius = n.radius })
    end
  end
  local d = n.divider
  if d == nil then
    return
  end
  local prev
  for _, k in ipairs(n.children) do
    if k then
      if prev and prev.kind ~= "spacer" and k.kind ~= "spacer" then
        if n.kind == "row" then
          local p = prev.x + prev.w + (k.x - prev.x - prev.w - d.t) // 2
          band(g, p, n.y + d.inset, d.t, n.h - 2 * d.inset, d.color, nil)
        else
          local p = prev.y + prev.h + (k.y - prev.y - prev.h - d.t) // 2
          band(g, n.x + d.inset, p, n.w - 2 * d.inset, d.t, d.color, nil)
        end
      end
      prev = k
    end
  end
end

-- what end_frame keeps for a container: it takes no focus and paints only its own look
local CONTAINER_KIND = { paint = paint_container }

-- The control record of node `n`, checked by its kind's field rules: the record, or nil and the error code.
local function control_of(st, n)
  local c = { kind = kinds[n.kind], id = n.id }
  if not c.kind.check(c, n.o, st) then
    return nil, INVALID
  end
  return c
end

-- Checks the fields of the controls under `n` without registering them (a hidden or zero-size subtree), but not
-- their boxes; returns as `register`.
local function check_only(st, n)
  if CONTAINERS[n.kind] then
    for _, child in ipairs(n.nodes or n.children) do
      if child then
        local _, err = check_only(st, child)
        if err then
          return nil, err
        end
      end
    end
  elseif CONTROLS[n.kind] then
    local _, err = control_of(st, n)
    return nil, err
  end
end

-- Registers the placed tree `n` in frame `f` in declaration order, containers before their children, checking each
-- control's own fields and box as it comes, as the reader does. A hidden node, or one with no width or no height, is
-- left out with everything in it, but its controls are still checked. Stops at the first refusal, keeping what was
-- registered before. Returns nothing, or nil and the error code.
local function register(st, f, n)
  if n.hidden or n.w <= 0 or n.h <= 0 then
    return check_only(st, n)
  end
  if CONTAINERS[n.kind] then
    if not reserve(f, 1) then
      return nil, "limit_exceeded"
    end
    if n.id then
      f.ids[n.id] = true
    end
    f.list[#f.list + 1] = { kind = CONTAINER_KIND, id = n.id, node = n }
    for _, child in ipairs(n.nodes or n.children) do
      if child then
        local _, err = register(st, f, child)
        if err then
          return nil, err
        end
      end
    end
    return
  end
  if not CONTROLS[n.kind] then
    return
  end
  local c, err = control_of(st, n)
  if c == nil then
    return nil, err
  end
  -- placed first: what a control costs may depend on its box
  c.x, c.y, c.w, c.h = n.x, n.y, n.w, n.h
  if not box_ok(c) then
    return nil, INVALID
  end
  if not reserve(f, cost_of(c)) then
    return nil, "limit_exceeded"
  end
  f.ids[c.id] = true
  f.list[#f.list + 1] = c
end

-- the `w` or `h` of an outermost container or a ui:layout rect: a whole number 1..MAX_COORD, or FIT from 2.15; nil
-- when invalid
local function outer_side(v)
  if is_side(v) and v <= MAX_COORD then
    return math.tointeger(v)
  end
  if v == FIT and api_at_least(15) then
    return FIT
  end
end

-- Places the tree `root` at `x, y` with the call's `w` and `h` (see root_size): true, or nil and the error code
-- when a grid finds a span longer than its `{ min }` columns, which only the laid-out width tells.
local function lay_out(root, x, y, w, h)
  local ok, err = pcall(function() place(root, x, y, root_size(root, w, h)) end)
  if ok then
    return true
  end
  if err == SPAN_TOO_LONG then
    return nil, INVALID
  end
  error(err, 0)
end

-- `ui:row(id, o)`, `ui:column(id, o)`, `ui:grid(id, o)` and `ui:stack(id, o)` (2.15). Returns nothing, or nil and
-- the error code.
local function declare_container(st, kind, id, o)
  local f = st.frame
  if f == nil or type(id) ~= "string" or id == "" or f.ids[id] or type(o) ~= "table" then
    return nil, INVALID
  end
  local w, h = outer_side(o.w), outer_side(o.h)
  if not (is_coord(o.x) and is_coord(o.y) and w and h) then
    return nil, INVALID
  end
  local x, y = math.tointeger(o.x), math.tointeger(o.y)
  local root, err
  if api_at_least(15) then
    root, err = layout_tree(st, f, id, kind, o, w)
    if root then
      local ok
      ok, err = lay_out(root, x, y, w, h)
      root = ok and root
    end
  else
    -- a tree cut at its first invalid child: what comes before it is registered first
    root, err = old_tree(st, f, id, kind, o, x, y, w, h)
  end
  if root then
    local _, refused = register(st, f, root)
    err = refused or err
  end
  if err then
    return nil, err
  end
end

for _, name in ipairs({ "row", "column" }) do
  methods[name] = function(self, id, o)
    local _, err = declare_container(state_of(self, name), name, id, o)
    if err then
      return nil, err
    end
  end
end

for _, name in ipairs({ "grid", "stack" }) do
  methods[name] = function(self, id, o)
    local st = state_of(self, name)
    if not api_at_least(15) then
      return nil, INVALID
    end
    local _, err = declare_container(st, name, id, o)
    if err then
      return nil, err
    end
  end
end

-- the box of node `n` for ui:layout, with its children's boxes at their positions; `ids` gathers them by id
local function result(n, ids)
  local t = { x = n.x, y = n.y, w = n.w, h = n.h, overflow = n.overflow }
  if CONTAINERS[n.kind] then
    for i = 1, #n.children do
      local c = n.children[i]
      t[i] = c and result(c, ids) or false
      if c and c.id then
        ids[c.id] = t[i]
      end
    end
  end
  return t
end

methods.layout = function(self, rect, spec)
  local st = state_of(self, "layout")
  if not api_at_least(15) or type(rect) ~= "table" or type(spec) ~= "table" then
    return nil, INVALID
  end
  local w, h = outer_side(rect.w), outer_side(rect.h)
  if not (is_coord(rect.x) and is_coord(rect.y) and w and h) then
    return nil, INVALID
  end
  local root, err = layout_tree(st, nil, nil, spec.kind, spec, w)
  if not root then
    return nil, err
  end
  local ok, bad = lay_out(root, math.tointeger(rect.x), math.tointeger(rect.y), w, h)
  if not ok then
    return nil, bad
  end
  local ids = {}
  local out = result(root, ids)
  for id, t in pairs(ids) do
    out[id] = t
  end
  return out
end
