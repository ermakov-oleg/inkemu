local T = require("conformance")

local R = { x = 0, y = 0, w = 100, h = 50 }

local function row(children, more)
  local s = { kind = "row", children = children }
  for k, v in pairs(more or {}) do s[k] = v end
  return s
end

local function col(children, more)
  local s = row(children, more)
  s.kind = "column"
  return s
end

-- each spec is refused with nil, "invalid_argument"
local SPECS = {
  min_over_max = row({ { w = 10, min_w = 20, max_w = 10 } }),
  columns_on_row = row({ { w = 10 } }, { columns = 2 }),
  justify_items_on_column = col({ { h = 10 } }, { justify_items = "start" }),
  divider_with_wrap = row({ { w = 10 } }, { wrap = true, divider = true }),
  column_span_child = row({ { w = 10, column_span = 1 } }),
  justify_self_child = row({ { w = 10, justify_self = "start" } }),
  id_x = row({ { id = "x", w = 10 } }),
  id_overflow = row({ { id = "overflow", w = 10 } }),
  id_repeated = row({ { id = "d", w = 10 }, { id = "d", w = 10 } }),
  id_empty = row({ { id = "", w = 10 } }),
  child_number = row({ 5 }),
  child_true = row({ true }),
  children_string = row("abc"),
  padding_256 = row({ { w = 10 } }, { padding = 256 }),
  padding_5_values = row({ { w = 10 } }, { padding = { 1, 2, 3, 4, 5 } }),
  gap_negative = row({ { w = 10 } }, { gap = -1 }),
  row_gap_float = row({ { w = 10 } }, { row_gap = 1.5 }),
  justify_unknown = row({ { w = 10 } }, { justify_content = "around" }),
  align_items_unknown = row({ { w = 10 } }, { align_items = "middle" }),
  align_content_baseline = row({ { w = 10 } }, { align_content = "baseline" }),
  wrap_number = row({ { w = 10 } }, { wrap = 1 }),
  fit_on_region = row({ { kind = "region", w = "fit" } }),
  fit_on_box = row({ { w = "fit" } }),
  percent_0 = row({ { w = "0%" } }),
  percent_101 = row({ { w = "101%" } }),
  percent_float = row({ { w = "1.5%" } }),
  aspect_with_h_fit = col({ { kind = "button", text = "a", aspect_ratio = 2, h = "fit" } }),
  aspect_zero = col({ { h = 10, aspect_ratio = 0 } }),
  no_size = row({ { id = "nosize" } }),
  spacer_spec = { kind = "spacer" },
  button_spec = { kind = "button", text = "a" },
  no_kind_spec = { children = {} },
  grow_1001 = row({ { grow = 1001 } }),
  shrink_float = row({ { w = 10, shrink = 0.5 } }),
  align_self_unknown = row({ { w = 10, align_self = "middle" } }),
  min_negative = row({ { w = 10, min_w = -1 } }),
  divider_width_0 = col({ { h = 10 } }, { divider = { line_width = 0 } }),
  divider_color_16 = col({ { h = 10 } }, { divider = { color = 16 } }),
  divider_inset_256 = col({ { h = 10 } }, { divider = { inset = 256 } }),
  background_16 = col({ { h = 10 } }, { background = 16 }),
  border_number = col({ { h = 10 } }, { border = 1 }),
  line_width_0 = col({ { h = 10 } }, { line_width = 0 }),
  border_color_16 = col({ { h = 10 } }, { border_color = 16 }),
  radius_256 = col({ { h = 10 } }, { radius = 256 }),
  skin_unknown = col({ { h = 10 } }, { skin = "nope" }),
  label_text_number = col({ { kind = "label", text = 5 } }),
  button_text_number_fit = col({ { kind = "button", text = 5, w = "fit", h = 40 } }, { align_items = "start" }),
  switch_note_number = col({ { kind = "switch", text = "a", note = 5 } }),
  tabs_without_items = col({ { kind = "tabs", h = 40 } }),
  unknown_kind = col({ { kind = "slider", h = 10 } }),
  w_0 = col({ { w = 0, h = 10 } }),
  w_float = col({ { w = 10.5, h = 10 } }),
}

local RECTS = {
  rect_w_0 = { x = 0, y = 0, w = 0, h = 10 },
  rect_w_percent = { x = 0, y = 0, w = "50%", h = 10 },
  rect_x_float = { x = 0.5, y = 0, w = 10, h = 10 },
  rect_no_h = { x = 0, y = 0, w = 10 },
}

function on_load(ctx)
  local ui = ctx.ui:create()
  local wrong, count = {}, 0
  local function try(name, rect, spec)
    count = count + 1
    local r = table.pack(pcall(ui.layout, ui, rect, spec))
    if not (r[1] and r.n == 3 and r[2] == nil and r[3] == "invalid_argument") then
      wrong[#wrong + 1] = name
    end
  end
  for name, spec in pairs(SPECS) do try(name, R, spec) end
  for name, rect in pairs(RECTS) do try(name, rect, row({ { w = 10 } })) end
  try("rect_not_table", nil, row({ { w = 10 } }))
  try("spec_not_table", R, "row")
  table.sort(wrong)
  T.value("cases", count)
  T.value("not_refused", wrong)
  -- refused before anything else is looked at: the same spec is fine with a valid rect
  T.call("valid", function() return type(ui:layout(R, row({ { w = 10 } }))) end)
  -- a text that is not a string is fine where it is not measured
  local function kind_of(rect, spec)
    local r = table.pack(ui:layout(rect, spec))
    return type(r[1]) == "table" and "table" or tostring(r[2])
  end
  T.value("label_text_number_with_h", kind_of(R, col({ { kind = "label", text = 5, h = 30 } })))
  T.value("button_text_table_stretched", kind_of(R, col({ { kind = "button", text = {}, h = 40 } })))
  -- a rect larger than 32767 either way
  T.value("rect_w_32767", kind_of({ x = 0, y = 0, w = 32767, h = 10 }, row({ { w = 10 } })))
  T.value("rect_w_32768", kind_of({ x = 0, y = 0, w = 32768, h = 10 }, row({ { w = 10 } })))
  T.value("rect_h_32768", kind_of({ x = 0, y = 0, w = 10, h = 32768 }, row({ { w = 10 } })))
end
