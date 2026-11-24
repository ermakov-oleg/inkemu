-- Helper for the §16 checks: records a frame's canvas calls as text, so a check can compare what ctx.ui drew with
-- calls it builds itself from the font metrics. Fonts show as @small, @medium, @large.
local L = {}

local OPS = { "rect", "line", "circle", "polygon", "text", "text_box", "image", "pattern" }

local function show(v)
  if type(v) == "number" then
    if math.type(v) == "integer" then return string.format("%d", v) end
    return string.format("%.17g", v)
  end
  if type(v) ~= "table" then return tostring(v) end
  for _, name in ipairs({ "small", "medium", "large" }) do
    if rawequal(v, ctx.fonts[name]) then return "@" .. name end
  end
  local keys = {}
  for k in pairs(v) do keys[#keys + 1] = k end
  table.sort(keys)
  for i, k in ipairs(keys) do keys[i] = k .. "=" .. show(v[k]) end
  return "{" .. table.concat(keys, ",") .. "}"
end

-- one call as the log writes it
function L.call(op, ...)
  local args = table.pack(...)
  for i = 1, args.n do args[i] = show(args[i]) end
  return op .. "(" .. table.concat(args, ",", 1, args.n) .. ")"
end

local wrapped = setmetatable({}, { __mode = "k" })

-- Starts a new log for `g`: from now on its drawing calls are appended to the returned list.
function L.start(g)
  local log = {}
  L.current = log
  if not wrapped[g] then
    wrapped[g] = true
    for _, op in ipairs(OPS) do
      local orig = g[op]
      g[op] = function(self, ...)
        local cur = L.current
        cur[#cur + 1] = L.call(op, ...)
        return orig(self, ...)
      end
    end
  end
  return log
end

-- a mark in the current log, such as a draw callback being called
function L.mark(text)
  L.current[#L.current + 1] = text
end

function L.text(log)
  return table.concat(log, " ")
end

return L
