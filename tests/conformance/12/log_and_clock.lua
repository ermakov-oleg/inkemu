local T = require("conformance")

function on_load(ctx)
  T.type("log", ctx.log)
  -- a 2.26-style ctx.log function must not hide the clock records below
  local log = type(ctx.log) == "table" and ctx.log or {}
  T.type("log.info", log.info)
  T.type("log.warn", log.warn)
  T.type("log.error", log.error)
  local t = ctx.clock:now()
  T.type("now.weekday", t.weekday)
  T.type("now.revision", t.revision)
  T.type("now.offset_minutes", t.offset_minutes)
  T.value("now.date_order_known", t.date_order == "ymd" or t.date_order == "mdy" or t.date_order == "dmy")
  T.value("now.date_sep_known", t.date_sep == "/" or t.date_sep == "-")
  T.type("now.wday", t.wday)
  T.type("now.yday", t.yday)
end
