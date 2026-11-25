-- ctx.ui (§16, 2.14): the progress bar. It is neither hit nor focused, and `enabled` is neither checked nor drawn.

local STYLES = { solid = true, stripe = true }

-- the most points one stripe polygon takes
local MAX_POINTS = 128

-- Stripes, in the filled area's own coordinates (u from its left, v from its top, 0..fw by 0..H): band `a` is
-- a <= u + v <= a + 3, for a = -1, 4, 9, ... while a <= fw + H - 2, so the band that would only nick the bottom
-- right corner is left out. Each band is the quad bottom left, top left, top right, bottom right, clipped by the
-- left, right, top and bottom edges in turn; a vertex inside an edge is kept, after the crossing that leads to it.

-- `pts` (a list of { u, v }) cut to the side where `inside(p)` holds; `cross(p, q)` is where p..q meets the edge
local function clip(pts, inside, cross)
  local out, n = {}, #pts
  if n == 0 then
    return out
  end
  local prev = pts[n]
  for _, cur in ipairs(pts) do
    local cin = inside(cur)
    if cin ~= inside(prev) then
      out[#out + 1] = cross(prev, cur)
    end
    if cin then
      out[#out + 1] = cur
    end
    prev = cur
  end
  return out
end

-- the edges only ever cut 45° or axis-aligned segments, so the crossings are whole numbers
local function at_u(k)
  return function(p, q)
    return { k, p[2] + (k - p[1]) * (q[2] - p[2]) // (q[1] - p[1]) }
  end
end

local function at_v(k)
  return function(p, q)
    return { p[1] + (k - p[2]) * (q[1] - p[1]) // (q[2] - p[2]), k }
  end
end

-- band `a` clipped to fw x H, without repeated points
local function band(a, fw, H)
  local pts = { { a - H, H }, { a, 0 }, { a + 3, 0 }, { a + 3 - H, H } }
  pts = clip(pts, function(p) return p[1] >= 0 end, at_u(0))
  pts = clip(pts, function(p) return p[1] <= fw end, at_u(fw))
  pts = clip(pts, function(p) return p[2] >= 0 end, at_v(0))
  pts = clip(pts, function(p) return p[2] <= H end, at_v(H))
  local out = {}
  for _, p in ipairs(pts) do
    local last = out[#out]
    if not (last and last[1] == p[1] and last[2] == p[2]) then
      out[#out + 1] = p
    end
  end
  while #out > 1 and out[1][1] == out[#out][1] and out[1][2] == out[#out][2] do
    out[#out] = nil
  end
  return out
end

-- `pts` turned to start at its first point that `key` ranks lowest; `key(p)` gives two numbers, compared in order
local function turned(pts, key)
  local b, k1, k2 = 1, key(pts[1])
  for i = 2, #pts do
    local a1, a2 = key(pts[i])
    if a1 < k1 or (a1 == k1 and a2 < k2) then
      b, k1, k2 = i, a1, a2
    end
  end
  local out = {}
  for i = 0, #pts - 1 do
    out[i + 1] = pts[(b - 1 + i) % #pts + 1]
  end
  return out
end

-- how long band `a` runs along row v of the area
local function run_at(a, v, fw)
  return math.min(a + 3 - v, fw) - math.max(a - v, 0)
end

-- The stripes of the area at x0, y0, fw by H. A band that runs along the bottom edge starts at its bottom left
-- point and joins the bottom polygon; one that runs along the top edge (and not the bottom) starts at its top right
-- point and joins the top polygon; any other is a polygon of its own, from its point highest on the left edge, drawn
-- at once. A joined polygon is drawn when the next band would take it past MAX_POINTS, and at the end, bottom first.
local function stripes(g, x0, y0, fw, H)
  local bottom, top = {}, {}
  local function flush(buf)
    if #buf > 0 then
      g:polygon(buf, { fill = true })
    end
    return {}
  end
  local function add(buf, pts)
    if #buf // 2 + #pts > MAX_POINTS then
      buf = flush(buf)
    end
    for _, p in ipairs(pts) do
      buf[#buf + 1] = x0 + p[1]
      buf[#buf + 1] = y0 + p[2]
    end
    return buf
  end
  for a = -1, fw + H - 2, 5 do
    local pts = band(a, fw, H)
    if run_at(a, H, fw) > 0 then
      bottom = add(bottom, turned(pts, function(p) return p[2] == H and 0 or 1, p[1] end))
    elseif run_at(a, 0, fw) > 0 then
      top = add(top, turned(pts, function(p) return p[2] == 0 and 0 or 1, -p[1] end))
    else
      flush(add({}, turned(pts, function(p) return p[1] == 0 and 0 or 1, p[2] end)))
    end
  end
  flush(bottom)
  flush(top)
end

kinds.progress = {
  check = function(c, o, st)
    local v = o.value
    if not (type(v) == "number" and v == v and (o.style == nil or STYLES[o.style])) then
      return false
    end
    local ok, skin = check_skin(st, o.skin)
    if not ok then
      return false
    end
    c.skin = skin
    c.value = math.max(0, math.min(1, v))
    c.style = o.style or "solid"
    return true
  end,
  -- with a skin: the whole bar in the normal look, then the leading part in the selected one; without: the 2 px
  -- frame, then the filled part 2 px inside it, solid or striped
  paint = function(g, c)
    if c.skin then
      return skin_bar(g, c.skin, c.x, c.y, c.w, c.h, math.floor(c.w * c.value + 0.5))
    end
    g:rect(c.x, c.y, c.w, c.h, { line_width = 2 })
    local fw, H = math.floor((c.w - 4) * c.value + 0.5), c.h - 4
    if fw <= 0 or H <= 0 then
      return
    end
    if c.style == "stripe" then
      stripes(g, c.x + 2, c.y + 2, fw, H)
    else
      g:rect(c.x + 2, c.y + 2, fw, H, { fill = true })
    end
  end,
}

methods.progress = control_method("progress")
