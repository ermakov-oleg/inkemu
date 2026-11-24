local T = require("conformance")
local L = require("canvas_log")
local objects = {}

local BUTTONS = {
  { "normal", {} }, { "round", { radius = 8 } }, { "selected", { selected = true } },
  { "emphasis", { emphasis = true } }, { "borderless", { border = false } },
}

-- each button's calls, unfocused and focused per style: box, filled box, outer ring, text in ink or paper
local PLAIN = { { "box", "ink" }, { "box", "ink" }, { "filled", "paper" }, { "filled", "paper" }, { "ink" } }
local FOCUSED = {
  fill = {
    { "filled", "paper" }, { "filled", "paper" }, { "filled", "ring", "paper" }, { "filled", "ring", "paper" },
    { "filled", "paper" },
  },
  outline = {
    { "box", "ring", "ink" }, { "box", "ring", "ink" }, { "filled", "ring", "paper" }, { "filled", "ring", "paper" },
    { "ring", "ink" },
  },
}

local function want(n, calls)
  local y, r = 20 + (n - 1) * 80, n == 2 and 8 or 0
  local out = {}
  for _, c in ipairs(calls) do
    if c == "box" or c == "filled" then
      out[#out + 1] = L.call("rect", 20, y, 200, 56, { fill = c == "filled", radius = r })
    elseif c == "ring" then
      out[#out + 1] = L.call("rect", 17, y - 3, 206, 62, { line_width = 2, radius = r > 0 and r + 3 or 0 })
    else
      out[#out + 1] = L.call("text_box", 20, y, 200, 56, "B", { align = "center", color = c == "ink" and 15 or 0,
        font = ctx.fonts.medium, overflow = "ellipsis", valign = "middle", wrap = "none" })
    end
  end
  return table.concat(out, " ")
end

-- the calls of a frame of `ui` with focus on button `n`, drawn because a key event came first
local function frame(ui, g, n)
  ui:focus(BUTTONS[n][1])
  ui:handle_event({ type = "key", key = "back", phase = "up", time_ms = 0 })
  ui:begin_frame(g)
  for i, b in ipairs(BUTTONS) do
    local o = { x = 20, y = 20 + (i - 1) * 80, w = 200, h = 56, text = "B" }
    for k, v in pairs(b[2]) do o[k] = v end
    ui:button(b[1], o)
  end
  local log = L.start(g)
  ui:end_frame()
  return L.text(log)
end

function on_load(ctx)
  objects.fill = ctx.ui:create()
  objects.outline = ctx.ui:create({ focus_style = "outline" })
end

local done = false

function on_draw(ctx, g)
  g:clear(0)
  if done then return end
  done = true
  for style, ui in pairs(objects) do
    for n, b in ipairs(BUTTONS) do
      local all = {}
      for i = 1, #BUTTONS do all[i] = want(i, i == n and FOCUSED[style][i] or PLAIN[i]) end
      T.value(style .. "." .. b[1], frame(ui, g, n) == table.concat(all, " "))
    end
  end
end
