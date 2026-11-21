//! Members with nothing to emulate beyond counters and fixed answers (0.1 `_install_stubs`).
use mlua::{IntoLuaMulti, Lua, Table};

use super::object::{method, new_object, Method};
use super::{arg, check_arg, fmt, int_value, nil, py_int};
use crate::emu::{with, Shared, State};

const CAPABILITIES: [(&str, bool); 8] = [
    ("dialog", true),
    ("frontlight", true),
    ("image_decode", true),
    ("image_transform", true),
    ("pattern", true),
    ("reader", true),
    ("ui", false),
    ("ttf", false),
];

fn counter(st: &Shared, bump: fn(&mut State)) -> Method {
    let st = st.clone();
    method(move |_, _| {
        with(&st, bump);
        Ok(nil())
    })
}

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let caps = lua.create_table()?;
    for (k, v) in CAPABILITIES {
        caps.raw_set(k, v)?;
    }
    ctx.raw_set("capabilities", caps)?;
    let i18n = lua.create_table()?;
    i18n.raw_set("locale", with(st, |s| s.cfg.locale.clone()))?;
    ctx.raw_set("i18n", i18n)?;

    let request = counter(st, |s| s.refresh_counts.0 += 1);
    let full = counter(st, |s| s.refresh_counts.1 += 1);
    let display = new_object(
        lua,
        "display",
        &[("request_refresh", request), ("full_refresh", full)],
    )?;
    ctx.raw_set("display", display)?;

    let get = {
        let st = st.clone();
        method(move |lua, _| int_value(with(&st, |s| s.frontlight)).into_lua_multi(lua))
    };
    let set = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            check_arg("set", 1, arg(&a, 0), "number")?;
            let n = py_int(arg(&a, 0))?;
            with(&st, |s| s.frontlight = n);
            true.into_lua_multi(lua)
        })
    };
    ctx.raw_set(
        "frontlight",
        new_object(lua, "frontlight", &[("get", get), ("set", set)])?,
    )?;

    let can_open = method(|lua, _| false.into_lua_multi(lua));
    let open = {
        let st = st.clone();
        method(move |lua, args| {
            let target = fmt(&args.into_iter().next().unwrap_or(mlua::Value::Nil));
            with(&st, |s| s.log_line(&format!("system:open {target}")));
            false.into_lua_multi(lua)
        })
    };
    ctx.raw_set(
        "system",
        new_object(lua, "system", &[("can_open", can_open), ("open", open)])?,
    )?;

    let info = {
        let st = st.clone();
        method(move |lua, _| {
            let t = lua.create_table()?;
            t.raw_set("free_heap", 1_048_576)?;
            t.raw_set("min_free_heap", 524_288)?;
            t.raw_set("uptime_ms", int_value(with(&st, |s| s.sched.clock.ms)))?;
            t.into_lua_multi(lua)
        })
    };
    ctx.raw_set("perf", new_object(lua, "perf", &[("info", info)])?)?;
    let collect = method(|_, _| Ok(nil()));
    let count = method(|lua, _| 256.0_f32.into_lua_multi(lua));
    ctx.raw_set(
        "gc",
        new_object(lua, "gc", &[("collect", collect), ("count", count)])?,
    )?;
    let extend = method(|lua, _| true.into_lua_multi(lua));
    ctx.raw_set("budget", new_object(lua, "budget", &[("extend", extend)])?)
}
