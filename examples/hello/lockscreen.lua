-- Lock screen for Hello: big clock, date and the tap count. Woken every interval_sec by the host.

local W, H = 480, 800
local DAYS = { 'Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday' }
local MONTHS = { 'January', 'February', 'March', 'April', 'May', 'June',
                 'July', 'August', 'September', 'October', 'November', 'December' }

function on_load(c) end

function on_draw(c, g)
  if not g then return end
  local ok, e = pcall(function()
    g:clear(0)
    local t = c.clock:now()
    g:rect(90, 240, W - 180, 120, { radius = 16 })
    g:text(W // 2, 300, string.format('%02d:%02d', t.hour, t.minute), { font = c.fonts.large, align = 'center', valign = 'middle' })
    g:text(W // 2, 400, string.format('%s, %d %s', DAYS[t.weekday], t.day, MONTHS[t.month]),
      { font = c.fonts.large, align = 'center' })
    g:line(120, 460, W - 120, 460)
    local taps = type(c.state) == 'table' and tonumber(c.state.taps) or 0
    g:text(W // 2, 490, 'Taps so far: ' .. taps, { font = c.fonts.medium, align = 'center' })
    local b = c.device:battery()
    g:text(W // 2, H - 60, 'Battery ' .. tostring(b.percent) .. '%', { font = c.fonts.small, align = 'center' })
  end)
  if not ok then g:text(10, 10, 'lock error: ' .. tostring(e):sub(1, 40)) end
  c.lock:done()
end
