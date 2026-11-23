-- Shared prelude of the conformance checks: records values as `@conformance <name> <json>` log lines.
-- Pure Lua on purpose: it must keep working when the ctx API under test is broken.

local T = {}

local function encode_string(s)
  if utf8.len(s) == nil then
    -- the log only carries text; binary must be hex-encoded by the check, this marks the mistake
    local hex = {}
    for i = 1, #s do hex[i] = string.format("%02x", string.byte(s, i)) end
    return '{"invalid_utf8_hex":"' .. table.concat(hex) .. '"}'
  end
  -- explicit range: %c also matches bytes of UTF-8 sequences in some C locales
  local out = string.gsub(s, '[\0-\31\127"\\]', function(c)
    if c == '"' then return '\\"' end
    if c == "\\" then return "\\\\" end
    if c == "\n" then return "\\n" end
    if c == "\t" then return "\\t" end
    if c == "\r" then return "\\r" end
    return string.format("\\u%04x", string.byte(c))
  end)
  return '"' .. out .. '"'
end

local function encode_number(v)
  if math.type(v) == "integer" then return string.format("%d", v) end
  if v ~= v then return "NaN" end
  if v == math.huge then return "Infinity" end
  if v == -math.huge then return "-Infinity" end
  local s = string.format("%.17g", v)
  if not string.find(s, "[%.eE]") then s = s .. ".0" end
  return s
end

local encode

local function encode_table(t, seen)
  if seen[t] then return '"<cycle>"' end
  seen[t] = true
  local count, keys = 0, {}
  for k in pairs(t) do
    count = count + 1
    keys[count] = k
  end
  local parts = {}
  local is_seq = true
  for i = 1, count do
    if t[i] == nil then is_seq = false break end
  end
  if is_seq then
    for i = 1, count do parts[i] = encode(t[i], seen) end
    seen[t] = nil
    return "[" .. table.concat(parts, ",") .. "]"
  end
  local names = {}
  for i, k in ipairs(keys) do names[i] = { tostring(k), k } end
  table.sort(names, function(a, b) return a[1] < b[1] end)
  for i, n in ipairs(names) do parts[i] = encode_string(n[1]) .. ":" .. encode(t[n[2]], seen) end
  seen[t] = nil
  return "{" .. table.concat(parts, ",") .. "}"
end

encode = function(v, seen)
  local ty = type(v)
  if v == nil then return "null" end
  if ty == "boolean" then return v and "true" or "false" end
  if ty == "number" then return encode_number(v) end
  if ty == "string" then return encode_string(v) end
  if ty == "table" then return encode_table(v, seen or {}) end
  return '{"$type":"' .. ty .. '"}'
end

T.encode = encode

local function emit(name, json)
  -- the runner reads `@conformance <name> <json>`, so a name must be one word
  if type(name) ~= "string" or name == "" or string.find(name, "%s") then
    error("conformance: record name must be a non-empty string without spaces, got " .. tostring(name), 3)
  end
  local c = ctx
  local line = "@conformance " .. tostring(name) .. " " .. json
  if type(c) ~= "table" and type(c) ~= "userdata" then
    error("conformance: no global ctx to log through", 0)
  end
  if type(c.log) == "table" then
    c.log:info(line)
  else
    c:log(line)
  end
end

local function strip_position(msg)
  if type(msg) ~= "string" then return tostring(msg) end
  return (string.gsub(msg, "^[^:\n]+:%d+: ", "", 1))
end

function T.value(name, v)
  emit(name, encode(v))
end

-- All results of f(...) with their count, or the error message without its chunk position.
function T.call(name, f, ...)
  local res = table.pack(pcall(f, ...))
  if not res[1] then
    emit(name, '{"err":' .. encode_string(strip_position(res[2])) .. ',"ok":false}')
    return
  end
  local parts = {}
  for i = 2, res.n do parts[i - 1] = encode(res[i]) end
  emit(name, '{"ok":true,"ret":[' .. table.concat(parts, ",") .. "]}")
end

-- "integer" or "float" for numbers, else type(v).
function T.type(name, v)
  local ty = type(v)
  if ty == "number" then ty = math.type(v) end
  emit(name, encode_string(ty))
end

-- Lowercase hex of a byte string, without relying on ctx.hex.
function T.hex(s)
  local out = {}
  for i = 1, #s do out[i] = string.format("%02x", string.byte(s, i)) end
  return table.concat(out)
end

return T
