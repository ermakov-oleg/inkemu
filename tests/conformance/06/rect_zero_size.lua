function on_draw(ctx, g)
  g:clear(0)
  g:rect(10, 10, 0, 10, { fill = true })
  g:rect(40, 10, -5, 10, { fill = true })
  g:rect(70, 10, 10, 0, { fill = true })
  g:rect(100, 10, 10, 10, { fill = true })
end
