function on_draw(ctx, g)
  g:clear(0)
  g:polygon({ 10, 10, 50, 10, 10, 50 }, { fill = true })
  g:polygon({ 100, 10, 140, 10, 140, 50 })
  g:polyline({ 200, 10, 240, 10, 240, 50 })
end
