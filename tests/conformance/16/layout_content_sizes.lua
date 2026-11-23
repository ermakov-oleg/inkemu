local T = require("conformance")
local B = require("layout_box")

local function height_of(font, text, w, opts)
  local _, h = font:layout(text, w, 32767, opts)
  return h
end

-- records `name` true when the box `t` is `w` × `h`, and the box itself for the log
local function is(name, t, w, h)
  T.value(name, t.w == w and t.h == h)
  T.value(name .. "_box", B.text(t))
end

function on_load(ctx)
  local ui = ctx.ui:create()
  local s, m, l = ctx.fonts.small, ctx.fonts.medium, ctx.fonts.large
  local lh = m.line_height
  local b = ui:layout({ x = 0, y = 0, w = 470, h = 800 }, { kind = "column", align_items = "start", children = {
    { kind = "label", text = "Hello", w = "fit", h = "fit" },
    { kind = "label", text = "ab\ncdef", w = "fit", h = "fit" },
    { kind = "button", text = "OK", w = "fit", h = "fit" },
    { kind = "button", text = "Go", font = l, w = "fit", h = "fit" },
    { kind = "switch", text = "Wi-Fi", w = "fit", h = "fit" },
    { kind = "switch", text = "Wi-Fi", note = "Note text", w = "fit", h = "fit" },
    { kind = "tabs", items = { "A", "BB", "CCC" }, gap = 4, w = "fit", h = "fit" },
    { kind = "label", text = "7", reserve = "000", w = "fit", h = "fit" },
    { kind = "label", text = "Hello", bold = true, w = "fit", h = "fit" },
    { kind = "button", text = "12", reserve = "8888", tabular = true, w = "fit", h = "fit" },
    { kind = "spacer", w = "fit", h = "fit" },
    { kind = "tabs", items = { "A", "BB", "CCC" }, gap = 4, bold = false, w = "fit", h = "fit" },
    { kind = "switch", text = "Wi-Fi", bold = false, w = "fit", h = "fit" },
  } })
  local lines = math.max(m:measure("ab"), m:measure("cdef"))
  is("label", b[1], m:measure("Hello"), height_of(m, "Hello", m:measure("Hello")))
  is("label_lines", b[2], lines, height_of(m, "ab\ncdef", lines))
  is("button", b[3], m:measure("OK") + 24, lh + 16)
  is("button_font", b[4], l:measure("Go") + 24, l.line_height + 16)
  is("label_reserve", b[8], math.max(m:measure("7"), m:measure("000")), height_of(m, "7", m:measure("000")))
  is("label_bold", b[9], m:measure("Hello", { bold = true }), height_of(m, "Hello", b[9].w, { bold = true }))
  local tab = { tabular = true }
  is("button_reserve", b[10], math.max(m:measure("12", tab), m:measure("8888", tab)) + 24, lh + 16)
  is("spacer", b[11], 0, 0)
  -- switch and tabs, plain and (from 2.16) measured bold
  local bold = { bold = true }
  T.value("bold_is_wider", m:measure("Wi-Fi", bold) > m:measure("Wi-Fi"))
  is("switch_plain", b[13], 8 + m:measure("Wi-Fi") + 12 + 64 + 8, math.max(34, lh) + 16)
  is("switch_bold", b[5], 8 + m:measure("Wi-Fi", bold) + 12 + 64 + 8, math.max(34, lh) + 16)
  is("switch_note", b[6], 8 + math.max(m:measure("Wi-Fi", bold), s:measure("Note text")) + 12 + 64 + 8,
    math.max(34, 2 * math.max(lh, s.line_height) + 4) + 16)
  local function tabs(opts)
    local widest = 0
    for _, item in ipairs({ "A", "BB", "CCC" }) do widest = math.max(widest, m:measure(item, opts)) end
    return 3 * (widest + 24) + 4 * 2
  end
  is("tabs_plain", b[12], tabs(nil), lh + 16)
  is("tabs_bold", b[7], tabs(bold), lh + 16)
  -- a fitted column: padding, gap, its widest child; a stretched child takes the inner width
  local f = ui:layout({ x = 10, y = 20, w = "fit", h = "fit" }, { kind = "column", padding = 8, gap = 4, children = {
    { kind = "button", text = "Save" }, { w = 30, h = 10 } } })
  local bw = m:measure("Save") + 24
  T.value("fit_column", f.w == 8 + bw + 8 and f.h == 8 + lh + 16 + 4 + 10 + 8 and f[1].w == bw and f[2].w == 30)
  T.value("fit_column_box", B.text(f))
  -- children that grow with no width make the row G × max(ceil(fit / grow)) wide
  local a, z = m:measure("A") + 24, m:measure("BBBB") + 24
  local g = ui:layout({ x = 0, y = 0, w = "fit", h = 50 }, { kind = "row", children = {
    { kind = "button", text = "A", grow = 1 }, { kind = "button", text = "BBBB", grow = 2 } } })
  T.value("grow_content", g.w == 3 * math.max(a, -((-z) // 2)))
  -- a percentage in a fitted column is not set while the content size is found, then takes the found width
  local p = ui:layout({ x = 0, y = 0, w = "fit", h = "fit" }, { kind = "column", children = {
    { kind = "button", text = "a", w = "50%" } } })
  local pw = m:measure("a") + 24
  T.value("percent_in_fit", p.w == pw and p[1].w == pw * 50 // 100)
  -- a label with no width in a column wraps to the inner width
  local text = "aaaa bbbb cccc dddd eeee ffff"
  local wrap = ui:layout({ x = 0, y = 0, w = 120, h = 400 }, { kind = "column", padding = 10, children = {
    { kind = "label", text = text, font = s } } })
  T.value("label_wraps", wrap[1].w == 100 and wrap[1].h == height_of(s, text, 100))
  -- text controls with no numeric height keep one line, even when the container is shorter
  local short = ui:layout({ x = 0, y = 0, w = 300, h = 10 }, { kind = "row", children = {
    { kind = "label", text = "Hi", grow = 1 }, { kind = "button", text = "B", w = 50 }, { w = 50 } } })
  T.value("one_line", short[1].h == lh and short[2].h == lh and short[3].h == 10 and short.overflow == 1)
end
