local T = require("conformance")
local L = require("canvas_log")
local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

local function refused(name, kind, o)
  T.call(name, function() return ui[kind](ui, name, o) end)
end

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  -- the 2.15 fields are ignored; every child is sized and they follow each other from the corner, gap apart
  T.call("column", function()
    return ui:column("old", { x = 0, y = 0, w = 200, h = 100, gap = 10, padding = 8, background = 5, border = true,
      children = {
        { kind = "button", id = "o1", text = "one", w = 150, h = 60 },
        { kind = "button", id = "o2", text = "two", w = 250, h = 60 },
      } })
  end)
  T.call("row", function()
    return ui:row("oldr", { x = 0, y = 300, w = 200, h = 50, gap = 5, justify_content = "end", children = {
      { kind = "button", id = "p1", text = "p1", w = 80, h = 50 },
      { kind = "label", id = "p2", text = "label", w = 80, h = 30 },
    } })
  end)
  refused("no_width", "row", { x = 0, y = 500, w = 300, h = 50, children = { { kind = "button", id = "e1", text = "one" } } })
  refused("spacer", "column", { x = 0, y = 500, w = 200, h = 300, children = {
    { kind = "button", id = "d1", text = "one", w = 200, h = 40 }, { kind = "spacer", w = 1, h = 1 } } })
  refused("fit", "column", { x = 0, y = 500, w = "fit", h = 100, children = {} })
  local log = L.start(g)
  ui:end_frame()
  local function button(x, y, w, h, text, fill)
    return L.call("rect", x, y, w, h, { fill = fill, radius = 0 }) .. " " .. L.call("text_box", x, y, w, h, text, {
      align = "center", color = fill and 0 or 15, font = ctx.fonts.medium, overflow = "ellipsis", valign = "middle", wrap = "none" })
  end
  local calls = L.text(log)
  T.value("placed", calls == table.concat({
    button(0, 0, 150, 60, "one", false), button(0, 70, 250, 60, "two", false), button(0, 300, 80, 50, "p1", false),
    L.call("text_box", 85, 300, 80, 30, "label", { font = ctx.fonts.medium, overflow = "ellipsis" }),
  }, " "))
  -- the second child is wider than its column and is drawn whole
  T.value("not_clipped", string.find(calls, L.call("rect", 0, 70, 250, 60, { fill = false, radius = 0 }), 1, true) ~= nil)
  T.call("grid", function() return ui:grid("g", { x = 0, y = 0, w = 10, h = 10, columns = 1, children = {} }) end)
  T.call("stack", function() return ui:stack("s", { x = 0, y = 0, w = 10, h = 10, children = {} }) end)
  T.call("layout", function() return ui:layout({ x = 0, y = 0, w = 10, h = 10 }, { kind = "row", children = {} }) end)
end
