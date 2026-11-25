local T = require("conformance")
local B = require("layout_box")

local R = { x = 0, y = 0, w = 480, h = 800 }

local function grid(more, children)
  local s = { kind = "grid", children = children or { {} } }
  for k, v in pairs(more) do s[k] = v end
  return s
end

local function stack(more, children)
  local s = grid(more, children)
  s.kind = "stack"
  return s
end

-- each spec is refused with nil, "invalid_argument"
local REFUSED = {
  auto_rows_table = grid({ columns = 2, auto_rows = { 10 } }),
  wrap_on_grid = grid({ columns = 2, wrap = true }),
  divider_on_grid = grid({ columns = 2, divider = true }),
  baseline_in_grid = grid({ columns = 2, align_items = "baseline" }),
  justify_items_baseline = grid({ columns = 2, justify_items = "baseline" }),
  span_past_columns = grid({ columns = 2 }, { { column_span = 3 } }),
  span_0 = grid({ columns = 2 }, { { column_span = 0 } }),
  grow_in_grid = grid({ columns = 2 }, { { grow = 1 } }),
  shrink_in_grid = grid({ columns = 2 }, { { shrink = 1 } }),
  columns_0 = grid({ columns = 0 }),
  columns_65 = grid({ columns = 65 }),
  columns_min_0 = grid({ columns = { min = 0 } }),
  no_columns = grid({}),
  track_0fr = grid({ columns = { "0fr" } }),
  track_1001fr = grid({ columns = { "1001fr" } }),
  track_0_percent = grid({ columns = { "0%" } }),
  track_0 = grid({ columns = { 0 } }),
  track_auto = grid({ columns = { "auto" } }),
  rows_empty = grid({ columns = 2, rows = {} }),
  gap_on_stack = stack({ gap = 4 }),
  columns_on_stack = stack({ columns = 2 }),
  justify_content_on_stack = stack({ justify_content = "center" }),
  baseline_in_stack = stack({ align_items = "baseline" }),
  grow_in_stack = stack({}, { { grow = 1 } }),
  span_in_stack = stack({}, { { column_span = 1 } }),
}

function on_load(ctx)
  local ui = ctx.ui:create()
  local function run(name, spec)
    T.value(name, B.run(ui, R, spec))
  end
  -- each child goes after the previous one's end, so the holes it leaves stay empty
  run("holes_stay", grid({ columns = 3, auto_rows = 40 }, {
    {}, { column_span = 3 }, {}, { column_span = 2 }, { column_span = 2 } }))
  -- a child spanning rows keeps its cells taken for the children after it
  run("row_spans", grid({ columns = 3, auto_rows = 40 }, {
    { column_span = 2 }, { row_span = 2 }, {}, { column_span = 3, row_span = 2 }, {} }))
  -- a spacer takes a cell like any child; its row is 0 high, as its minimum is 0
  run("spacer", grid({ columns = 2 }, { { kind = "spacer", column_span = 2 }, {} }))
  -- hidden and false children take no cell
  run("hidden_and_false", grid({ columns = 2, auto_rows = 30 }, { { hidden = true }, false, {}, {} }))
  local wrong, count = {}, 0
  for name, spec in pairs(REFUSED) do
    count = count + 1
    local r = table.pack(pcall(ui.layout, ui, R, spec))
    if not (r[1] and r.n == 3 and r[2] == nil and r[3] == "invalid_argument") then
      wrong[#wrong + 1] = name
    end
  end
  table.sort(wrong)
  T.value("cases", count)
  T.value("not_refused", wrong)
end
