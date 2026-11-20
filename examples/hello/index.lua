-- Hello: the smallest useful InkApp. Clock, battery, a tap counter kept in ctx.state.
-- Screen 480x800 portrait, 1 bpp. Host callbacks: on_load, on_resume, on_draw, on_input, on_leave.

local W, H = 480, 800
local BTN = { x = 90, y = 560, w = 300, h = 80 }
local RESET = { x = 140, y = 680, w = 200, h = 56 }

local S                 -- ctx.state, persisted by the host between runs
local timer
local last_err

local function refresh() pcall(function() ctx:invalidate() end) end

local function font(name)
  local ok, f = pcall(function() return ctx.fonts[name] end)
  return ok and f or nil
end

local function now()
  local ok, t = pcall(function() return ctx.clock:now() end)
  if ok and type(t) == 'table' then return t end
end

local function battery()
  local ok, b = pcall(function() return ctx.device:battery() end)
  if ok and type(b) == 'table' then return b end
end

local function inside(b, x, y)
  return x >= b.x and x < b.x + b.w and y >= b.y and y < b.y + b.h
end

-- redraw on the next minute boundary, so the clock stays right without waking every second
local function schedule()
  pcall(function() if timer then timer:cancel() end end)
  local t = now()
  local sec = t and tonumber(t.second) or 0
  local ok, h = pcall(function()
    return ctx:after((60 - sec) * 1000 + 200, function() refresh(); schedule() end)
  end)
  timer = ok and h or nil
end

local function button(g, b, label, f)
  g:rect(b.x, b.y, b.w, b.h)
  g:rect(b.x + 1, b.y + 1, b.w - 2, b.h - 2)
  g:text(b.x + b.w // 2, b.y + b.h // 2, label, { font = f, align = 'center', valign = 'middle' })
end

function on_load(c)
  c = c or ctx
  if type(c.state) ~= 'table' then c.state = {} end
  S = c.state
  S.taps = tonumber(S.taps) or 0
  schedule()
end

function on_resume(c)
  refresh(); schedule()
end

function on_draw(c, g)
  if not g then return end
  local ok, e = pcall(function()
    g:clear(0)
    local L, M, Sm = font('large'), font('medium'), font('small')

    g:text(W // 2, 40, 'Hello, X4 Pro!', { font = L, align = 'center' })
    g:line(24, 90, W - 24, 90)

    local t = now()
    if t then
      g:text(W // 2, 160, string.format('%02d:%02d', t.hour or 0, t.minute or 0), { font = L, align = 'center' })
    else
      g:text(W // 2, 160, 'clock not set', { font = M, align = 'center' })
    end

    local b = battery()
    if b then
      local s = 'Battery ' .. tostring(b.percent or '?') .. '%' .. (b.charging and ' (charging)' or '')
      g:text(W // 2, 230, s, { font = M, align = 'center' })
    end

    g:text(W // 2, 380, 'Taps: ' .. tostring(S and S.taps or 0), { font = L, align = 'center' })

    button(g, BTN, 'Tap me', L)
    button(g, RESET, 'Reset', M)
    g:text(W // 2, H - 40, 'Home pad: leave app', { font = Sm, align = 'center' })

    if last_err then g:text(10, H - 80, 'err: ' .. tostring(last_err):sub(1, 50), { font = Sm }) end
  end)
  if not ok then pcall(function() g:text(10, 10, 'draw error: ' .. tostring(e):sub(1, 40)) end) end
end

function on_input(c, ev)
  if type(ev) ~= 'table' then return false end
  local ok, handled = pcall(function()
    if ev.type == 'pointer' and ev.phase == 'up' and ev.gesture == 'tap' then
      local x, y = ev.x or 0, ev.y or 0
      if inside(BTN, x, y) then S.taps = S.taps + 1; refresh(); return true end
      if inside(RESET, x, y) then S.taps = 0; refresh(); return true end
      return true
    end
    if ev.type == 'key' and ev.phase ~= 'up' and (ev.key == 'up' or ev.key == 'right') then
      S.taps = S.taps + 1; refresh(); return true
    end
    return false
  end)
  if not ok then last_err = handled; refresh(); return true end
  return handled
end

function on_leave(c)
  pcall(function() if timer then timer:cancel() end end)
end
