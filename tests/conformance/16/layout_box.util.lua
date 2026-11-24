-- Helper for the §16 layout checks: a ui:layout result as short text, so a check can record whole trees.
local B = {}

-- "x,y,w,h", with "!n" for an overflow at child n and the children in brackets; false for a skipped child
function B.text(t)
  if t == false then
    return "false"
  end
  if type(t) ~= "table" then
    return tostring(t)
  end
  local out = string.format("%s,%s,%s,%s", t.x, t.y, t.w, t.h)
  if t.overflow ~= nil then
    out = out .. "!" .. tostring(t.overflow)
  end
  if #t > 0 then
    local kids = {}
    for i = 1, #t do
      kids[i] = B.text(t[i])
    end
    out = out .. " [" .. table.concat(kids, " | ") .. "]"
  end
  return out
end

-- the widths (or another field) of the children, "a,b,c"
function B.list(t, field)
  local out = {}
  for i = 1, #t do
    out[i] = tostring(t[i] and t[i][field or "w"])
  end
  return table.concat(out, ",")
end

-- the result of a ui:layout call as text, or its error code
function B.run(ui, rect, spec)
  local b, err = ui:layout(rect, spec)
  if b == nil then
    return "nil," .. tostring(err)
  end
  return B.text(b)
end

return B
