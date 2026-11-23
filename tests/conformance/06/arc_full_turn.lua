function on_draw(ctx, g)
  g:clear(0)
  g:arc(100, 100, 50, 0, 360, { line_width = 3 })
  g:arc(250, 100, 50, 10, 730, { line_width = 3 })
  g:arc(400, 100, 50, 45, 45, { line_width = 3 })
end
