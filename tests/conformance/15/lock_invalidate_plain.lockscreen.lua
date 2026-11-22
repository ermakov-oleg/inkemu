function on_load(ctx)
  ctx.net:request({ url = "https://x.test/lock" }, function(ctx, res)
    if ctx.lock.reason == "timer" then ctx:invalidate() end
  end)
end

function on_draw(ctx, g) g:rect(10, 10, 20, 20, { fill = true }) end
