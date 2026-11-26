-- ctx.ui (§16, 2.9): text fields, `ui:editing` and `ui:stop_edit`. A field draws the text the script declares;
-- activating it edits that text with the host's docked input, `ctx.input:edit`, one edit per UI object.
--
-- `st.edit` is the object's open edit, { id, c, ed }: `c` is the field's record from the frame the edit started
-- on. It is cleared before the library calls out (`ed:close()`, the script's callbacks, the next edit), so what
-- those do sees no edit of this object.

local MARGIN = 8

-- the text area of field `c`: its box less the margin, where the text is drawn and the host edits
local function inner(c)
  return c.x + MARGIN, c.y + MARGIN, c.w - 2 * MARGIN, c.h - 2 * MARGIN
end

kinds.field = {
  focusable = true,
  check = function(c, o)
    -- the edit options, text and look are not checked: the host checks the options when editing starts (s26)
    if not (optional(o.enabled, "boolean") and check_nav(o.nav)) then
      return false
    end
    c.enabled = o.enabled ~= false
    c.nav = o.nav
    c.text, c.font, c.radius = o.text, o.font, o.radius
    c.placeholder, c.max_chars, c.input_kind = o.placeholder, o.max_chars, o.kind
    c.multiline, c.enter = o.multiline, o.enter
    c.on_change, c.on_end = o.on_change, o.on_end
    return true
  end,
  paint = function(g, c, look)
    g:rect(c.x, c.y, c.w, c.h, { line_width = look.focused and 2 or 1, radius = c.radius or 0 })
    local text, color = c.text, nil
    if text == nil or text == "" then
      text, color = c.placeholder or "", 8
    elseif c.input_kind == "secret" and type(text) == "string" then
      text = string.rep("*", utf8.len(text) or #text)
    end
    local opts = { font = c.font or host.fonts.medium, color = color }
    if c.multiline then
      opts.overflow, opts.valign, opts.wrap = "clip", "top", "char"
    else
      opts.overflow, opts.valign, opts.wrap = "ellipsis", "middle", "none"
    end
    local x, y, w, h = inner(c)
    g:text_box(x, y, w, h, text, opts)
    dots(g, c)
  end,
}

methods.field = control_method("field")

local function call(f, ...)
  if type(f) == "function" then
    f(...)
  end
end

-- Ends the object's edit without its on_end; returns what `ed:close()` returns.
local function close_edit(st)
  local e = st.edit
  st.edit = nil
  return e.ed:close()
end

-- the first enabled field after field `id` among the last closed frame's targets
local function next_field(st, id)
  local after = false
  for _, t in ipairs(st.targets) do
    if after and t.kind == kinds.field and t.enabled then
      return t
    end
    after = after or t.id == id
  end
end

local open_edit

-- The host's on_end of edit `e`: the field's own on_end, then with `enter = "next"` and the confirm key the next
-- field edits and takes focus.
local function ended(st, e, ctx, text, how)
  if st.edit ~= e then
    return
  end
  st.edit = nil
  local c = e.c
  call(c.on_end, ctx, text, how)
  if how == "enter" and c.enter == "next" then
    local t = next_field(st, c.id)
    if t then
      set_focus(st, t.id, nil)
      open_edit(st, t)
    end
  end
end

-- Starts editing field `c`: `c.id, "edit"`, or `c.id, "activate"` when the host refuses. The options go to the host
-- as declared, with the library's own callbacks; a host that raises on one raises out of the caller.
open_edit = function(st, c)
  local e = { id = c.id, c = c }
  local x, y, w, h = inner(c)
  local ed = host.input:edit({
    text = c.text, placeholder = c.placeholder, max_chars = c.max_chars, kind = c.input_kind,
    multiline = c.multiline, enter = c.enter, rect = { x = x, y = y, w = w, h = h },
    on_change = function(ctx, text)
      if st.edit == e then
        call(c.on_change, ctx, text)
      end
    end,
    on_end = function(ctx, text, how)
      ended(st, e, ctx, text, how)
    end,
  })
  if not ed then
    return c.id, "activate"
  end
  e.ed = ed
  st.edit = e
  return c.id, "edit"
end

-- What activating field `c` returns: the field already edited stays so; another one's edit ends first.
function activate_field(st, c)
  local e = st.edit
  if e and e.id == c.id then
    return c.id, "edit"
  end
  if e then
    close_edit(st)
  end
  return open_edit(st, c)
end

-- At end_frame: the edit ends when this frame declares no field with its id.
function drop_lost_edit(st)
  local e = st.edit
  if e == nil then
    return
  end
  local c = st.owners[e.id]
  if not (c and c.kind == kinds.field) then
    close_edit(st)
  end
end

methods.editing = function(self)
  local e = state_of(self, "editing").edit
  if e then
    return e.id
  end
  return nil
end

methods.stop_edit = function(self)
  local st = state_of(self, "stop_edit")
  if st.edit == nil then
    return nil
  end
  return (close_edit(st))
end
