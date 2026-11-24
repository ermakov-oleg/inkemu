function on_draw(ctx, g)
  g:clear(0)
  g:rect(10, 10, 20, 20, { fill = true, radius = 5 })
  g:rect(50, 10, 10, 20, { fill = true, radius = 200 })
end
