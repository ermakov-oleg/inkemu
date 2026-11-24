//! Members with nothing to emulate beyond fixed answers (0.1 `_install_stubs`).
use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value};

use super::object::{method, new_object};
use super::{arg, check_arg, fmt, int_value, nil, py_int, request_frame, soft_fail};
use crate::emu::{with, Shared};

/// What the emulator emulates; the device (26112001) also reports `ble`, `identity`, `nn` and `ui` as true.
const CAPABILITIES: [(&str, bool); 11] = [
    ("ble", false),
    ("dialog", true),
    ("frontlight", true),
    ("identity", false),
    ("image_decode", true),
    ("image_transform", true),
    ("nn", false),
    ("pattern", true),
    ("reader", true),
    ("ui", true),
    ("ttf", false),
];

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let caps = lua.create_table()?;
    for (k, v) in CAPABILITIES {
        caps.raw_set(k, v)?;
    }
    ctx.raw_set("capabilities", caps)?;
    let i18n = lua.create_table()?;
    i18n.raw_set("locale", with(st, |s| s.cfg.locale.clone()))?;
    ctx.raw_set("i18n", i18n)?;

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
    let reset = {
        let st = st.clone();
        method(move |_, _| {
            with(&st, |s| s.frontlight = 0);
            Ok(nil())
        })
    };
    ctx.raw_set(
        "frontlight",
        new_object(
            lua,
            "frontlight",
            &[("get", get), ("set", set), ("reset", reset)],
        )?,
    )?;

    let can_open = method(|lua, _| false.into_lua_multi(lua));
    // the strings the device accepts are unknown: every one is refused, as an unknown one is there
    let act = {
        let st = st.clone();
        method(move |lua, args| {
            let Some(Value::String(what)) = args.into_iter().next() else {
                return Err(mlua::Error::runtime(
                    "bad argument #1 to 'act' (string expected)",
                ));
            };
            if what == "refresh" {
                with(&st, |s| request_frame(s, true));
                return Ok(MultiValue::new());
            }
            let what = what.to_string_lossy();
            with(&st, |s| {
                s.log_line(&format!("system:act {what}: not emulated"))
            });
            soft_fail(lua, "invalid_argument")
        })
    };
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
        new_object(
            lua,
            "system",
            &[("act", act), ("can_open", can_open), ("open", open)],
        )?,
    )?;

    // the timings and the image cache measure the device's renderer, which inkemu does not have: always 0
    let info = {
        let st = st.clone();
        method(move |lua, _| {
            let p = with(&st, |s| s.perf);
            let t = lua.create_table()?;
            for k in ["tick_us", "draw_us", "asset_hit", "asset_miss"] {
                t.raw_set(k, 0)?;
            }
            t.raw_set("cmd_count", int_value(p.cmd_count as i64))?;
            t.raw_set("image_count", int_value(p.image_count as i64))?;
            t.raw_set("draw_frame", int_value(p.draw_frame as i64))?;
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
