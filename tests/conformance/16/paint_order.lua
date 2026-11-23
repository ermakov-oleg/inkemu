local ui

function on_load(ctx)
  ui = ctx.ui:create()
end

function on_draw(ctx, g)
  g:clear(0)
  ui:begin_frame(g)
  ui:button("under", { x = 20, y = 20, w = 200, h = 60, text = "", selected = true })
  -- drawn by the script after the button was declared, yet it ends up below the button's fill
  g:rect(20, 20, 200, 60, { fill = true, color = 0 })
  ui:button("first", { x = 20, y = 100, w = 200, h = 60, text = "", selected = true })
  ui:region("second", { x = 20, y = 100, w = 200, h = 60, draw = function(g2, r)
    g2:rect(r.x, r.y, r.w, r.h, { fill = true, color = 0 })
  end })
  ui:end_frame()
end
