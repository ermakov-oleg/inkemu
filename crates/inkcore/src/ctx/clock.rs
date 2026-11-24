//! `ctx.clock` (0.1 `_install_clock`), with the `clock:now()` fields of firmware 26112001.
use mlua::{IntoLuaMulti, Lua, Table};

use super::int_value;
use super::object::{method, new_object};
use crate::clock::civil;
use crate::emu::{with, Shared};

const DAY_MS: i64 = 86_400_000;

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let now = {
        let st = st.clone();
        method(move |lua, _| {
            let (local, offset, revision) = with(&st, |s| {
                (
                    s.cfg.start_local_ms + s.sched.clock.ms,
                    i64::from(s.cfg.utc_offset_sec),
                    s.clock_revision,
                )
            });
            let (y, mo, d, h, mi, sec, _) = civil(local);
            let days = local.div_euclid(DAY_MS);
            // 1970-01-01 was a Thursday; 0 is Sunday
            let weekday = (days + 4).rem_euclid(7);
            let epoch = local.div_euclid(1000) - offset;
            let t = lua.create_table()?;
            t.raw_set("year", y)?;
            t.raw_set("month", mo)?;
            t.raw_set("day", d)?;
            t.raw_set("hour", h)?;
            t.raw_set("minute", mi)?;
            t.raw_set("second", sec)?;
            t.raw_set("weekday", weekday)?;
            t.raw_set("epoch_sec", int_value(epoch))?;
            t.raw_set("local_sec", int_value(epoch + offset))?;
            t.raw_set("offset_minutes", int_value(offset.div_euclid(60)))?;
            t.raw_set("valid", true)?;
            t.raw_set("hour12", false)?;
            t.raw_set("date_order", "dmy")?;
            t.raw_set("date_sep", "/")?;
            t.raw_set("revision", revision)?;
            t.into_lua_multi(lua)
        })
    };
    let millis = {
        let st = st.clone();
        method(move |lua, _| int_value(with(&st, |s| s.sched.clock.ms)).into_lua_multi(lua))
    };
    let clock = new_object(lua, "clock", &[("now", now), ("millis", millis)])?;
    ctx.raw_set("clock", clock)
}
