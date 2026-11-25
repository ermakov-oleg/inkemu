-- ctx.ui (§16), the core: UI objects, `create` and the method table the other files fill in.
-- All files of the library share one private environment; `host` comes from the Rust glue.
-- Call `string.x(s)`, never `s:x()`: the string metatable indexes the app's own, patchable `string` table.
-- `host.fonts` is the app's live `ctx.fonts`, so an app that patches it changes only its own UI.

-- a UI object is an empty table; its state lives here, out of the app's reach
objects = setmetatable({}, { __mode = "k" })
methods = {}
object_meta = { __index = methods, __metatable = false }

-- package's manifest declares API 2.<minor> or later
function api_at_least(minor)
  return host.api_minor >= minor
end

-- the state of the UI object `self`, or a Lua error at the app's call of method `name`
function state_of(self, name)
  local s = objects[self]
  if s == nil then
    error("bad argument #1 to '" .. name .. "' (call ui:" .. name .. " with a colon)", 3)
  end
  return s
end

-- every method of §16 exists from the start; the files that implement one replace its stub
for _, name in ipairs({
  "begin_frame", "end_frame", "handle_event", "focus", "focused", "layout", "skin",
  "label", "button", "image", "region",
  "radio",
  "row", "column", "grid", "stack",
}) do
  methods[name] = function(self)
    state_of(self, name)
    error("ui:" .. name .. " is not implemented in inkemu yet", 2)
  end
end

local FOCUS_STYLES = { fill = true, outline = true }

-- whether the device has a touchscreen, as `ctx.input.pointer` says when the object is created
local function touch()
  local input = host.input
  if type(input) == "table" and input.pointer ~= nil then
    return input.pointer == true
  end
  return host.pointer
end

function create(opts)
  if opts == nil then
    opts = {}
  elseif type(opts) ~= "table" then
    return nil, "invalid_argument"
  end
  local focus_style = nil
  if api_at_least(16) then
    focus_style = opts.focus_style
  end
  if focus_style == nil then
    focus_style = "fill"
  elseif not FOCUS_STYLES[focus_style] then
    return nil, "invalid_argument"
  end
  local skins = check_skins(opts.skins == nil and {} or opts.skins)
  if skins == nil then
    return nil, "invalid_argument"
  end
  local obj = setmetatable({}, object_meta)
  objects[obj] = {
    focus_style = focus_style,
    -- the kept focus (id and part), and whether focus is drawn: with no touch input it is drawn from creation
    focus = nil,
    part = nil,
    keys = not touch(),
    -- an `ok` down came on the kept focus; cleared by its `ok` up and by any focus move
    pressed = nil,
    -- the open frame, and the targets of the last frame that was closed (events and arrows work on these) with
    -- the controls that own them by id
    frame = nil,
    targets = {},
    owners = {},
    -- the skins of `create` by name, checked (skin.lua)
    skins = skins,
    -- the field being edited (field.lua)
    edit = nil,
  }
  return obj
end
