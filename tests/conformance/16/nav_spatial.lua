local T = require("conformance")
local ui

local DIRS = { "up", "down", "left", "right" }

local function key(name)
  return ui:handle_event({ type = "key", key = name, phase = "down", time_ms = 0 })
end

-- one frame of `{id, x, y, w, h[, enabled]}` buttons (`kind = "region"` for a region)
local function frame(g, boxes)
  ui:begin_frame(g)
  for _, b in ipairs(boxes) do
    local o = { x = b[2], y = b[3], w = b[4], h = b[5], enabled = b[6], text = b[1] }
    if b.kind == "region" then
      ui:region(b[1], o)
    else
      ui:button(b[1], o)
    end
  end
  ui:end_frame()
end

-- where `dir` from `from` goes: the id returned with focus, or "-" for nil, nil (focus then stays on `from`)
local function move(from, dir)
  ui:focus(from)
  local id, action = key(dir)
  if id == nil and action == nil and ui:focused() == from then
    return "-"
  end
  return tostring(id) .. (action == "focus" and "" or "?" .. tostring(action))
end

-- `dir` from F in a frame of F (200, 100, 80 x 60) and `others`
local function case(g, name, dir, others)
  local boxes = { { "F", 200, 100, 80, 60 } }
  for _, b in ipairs(others) do boxes[#boxes + 1] = b end
  frame(g, boxes)
  T.value(name, move("F", dir))
end

function on_load(ctx)
  ui = ctx.ui:create()
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then return end
  done = true
  local grid = {}
  for i = 1, 9 do
    grid[i] = { "g" .. i, 40 + (i - 1) % 3 * 140, 300 + (i - 1) // 3 * 100, 100, 60 }
  end
  frame(g, grid)
  for i = 1, 9 do
    local row = {}
    for n, dir in ipairs(DIRS) do row[n] = dir .. "=" .. move("g" .. i, dir) end
    T.value("grid.g" .. i, table.concat(row, " "))
  end
  case(g, "overlap_beats_gap", "down", { { "Q", 320, 200, 80, 60 }, { "P", 220, 400, 80, 60 } })
  case(g, "smaller_gap", "down", { { "P1", 200, 250, 80, 60 }, { "P2", 240, 180, 80, 60 } })
  case(g, "cross_distance", "down", { { "R2", 60, 200, 80, 60 }, { "R1", 320, 200, 80, 60 } })
  case(g, "declared_first", "down", { { "R2", 80, 200, 80, 60 }, { "R1", 320, 200, 80, 60 } })
  case(g, "top_edge_above", "down", { { "T", 300, 50, 80, 300 }, { "far", 0, 700, 80, 60 } })
  case(g, "gap_zero", "right", { { "O", 260, 120, 80, 60 }, { "Far", 300, 100, 80, 60 } })
  case(g, "left_edge_before", "right", { { "W", 190, 300, 200, 60 }, { "N", 400, 600, 50, 50 } })
  case(g, "left_nearest_overlapping", "left",
    { { "L1", 0, 100, 50, 60 }, { "L2", 100, 130, 50, 60 }, { "L3", 120, 300, 50, 60 } })
  case(g, "disabled_skipped", "down", { { "E1", 200, 200, 80, 60, false }, { "E2", 200, 300, 80, 60 } })
  case(g, "region", "down", { { "RG", 200, 200, 80, 60, kind = "region" } })
  case(g, "nothing_that_way", "up", { { "D", 200, 200, 80, 60 } })
  -- no focus yet: an arrow takes the first available control
  ui = ctx.ui:create()
  frame(g, { { "x", 0, 0, 50, 50, false }, { "y", 0, 100, 50, 50 }, { "z", 0, 200, 50, 50 } })
  ui:focus("ghost")
  T.call("unknown_focus", function() return key("up") end)
end
