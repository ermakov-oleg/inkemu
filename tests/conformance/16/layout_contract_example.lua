local T = require("conformance")

function on_load(ctx)
  local ui = ctx.ui:create()
  local r = table.pack(ui:layout({ x = 0, y = 0, w = 480, h = 800 }, { kind = "column", padding = 24, gap = 12, children = {
    { id = "head", h = 64 },
    { id = "body", grow = 1 },
    { kind = "row", h = 64, gap = 12, children = { { id = "a", grow = 1 }, { id = "b", grow = 1 } } },
  } }))
  local b = r[1]
  local function box(t)
    return { t.x, t.y, t.w, t.h }
  end
  T.value("returns", r.n)
  T.value("root", box(b))
  T.value("head", box(b.head))
  T.value("body", box(b.body))
  T.value("row", box(b[3]))
  T.value("a", box(b.a))
  T.value("b", box(b.b))
  T.value("b_is_the_numbered_table", rawequal(b[3][2], b.b) and rawequal(b[1], b.head))
  -- ids are keys of the returned table only, not of the nested ones
  T.value("nested_has_no_id_keys", b[3].a == nil and b[3].b == nil)
  T.value("no_overflow", b.overflow == nil and b[3].overflow == nil)
end
