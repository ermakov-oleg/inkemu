//! `ctx.fonts` (0.1 `_install_fonts`).
use mlua::{IntoLuaMulti, Lua, Table, Value};

use super::object::{method, new_object};
use super::{arg, check_arg, fmt};
use crate::draw::text::FONT_NAMES;
use crate::emu::{with, Shared};

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let fonts = with(st, |s| s.fonts.clone());
    let mut handles = vec![];
    for name in FONT_NAMES {
        let f = fonts.clone();
        let measure = method(move |lua, args| {
            let s = fmt(args.iter().next().unwrap_or(&Value::Nil));
            f.measure(name, &s).into_lua_multi(lua)
        });
        let h = new_object(lua, &format!("fonts.{name}"), &[("measure", measure)])?;
        h.raw_set("name", name)?;
        h.raw_set("line_height", fonts.line_height(name))?;
        h.raw_set("ascent", fonts.ascent(name))?;
        handles.push((name, h));
    }
    let size = {
        let handles = handles.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            check_arg("size", 1, arg(&a, 0), "number")?;
            let px = match arg(&a, 0) {
                Value::Integer(i) => f64::from(*i),
                Value::Number(n) => f64::from(*n),
                _ => 0.0,
            };
            // the first of equally near sizes wins, as Python's min()
            let mut best = &handles[0];
            for h in &handles[1..] {
                let d = |name| (f64::from(fonts.line_height(name)) - px).abs();
                if d(h.0) < d(best.0) {
                    best = h;
                }
            }
            best.1.clone().into_lua_multi(lua)
        })
    };
    let table = new_object(lua, "fonts", &[("size", size)])?;
    for (name, h) in handles {
        table.raw_set(name, h)?;
    }
    ctx.raw_set("fonts", table)
}
