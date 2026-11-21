//! `ctx.screen` (0.1 `_install_screen`).
use mlua::{IntoLuaMulti, Lua, Table, Value};

use super::object::{add_methods, method, new_object};
use super::soft_fail;
use crate::emu::{with, Shared};

fn orientation(w: u32, h: u32) -> &'static str {
    if w > h {
        "landscape"
    } else {
        "portrait"
    }
}

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let (w, h) = with(st, |s| (s.canvas.w, s.canvas.h));
    let screen = new_object(lua, "screen", &[])?;
    screen.raw_set("width", w)?;
    screen.raw_set("height", h)?;
    screen.raw_set("orientation", orientation(w, h))?;
    screen.raw_set("gray_levels", 2)?;
    screen.raw_set("gray_still", 4)?;
    let set_orientation = {
        let (st, screen) = (st.clone(), screen.clone());
        method(move |lua, args| {
            let (name, size) = match args.iter().next() {
                Some(Value::String(s)) if s.as_bytes() == b"portrait" => ("portrait", (480, 800)),
                Some(Value::String(s)) if s.as_bytes() == b"landscape" => ("landscape", (800, 480)),
                _ => return soft_fail(lua, "invalid_argument"),
            };
            let changed = with(&st, |s| {
                let changed = name != orientation(s.canvas.w, s.canvas.h);
                if changed {
                    s.canvas.resize(size.0, size.1);
                    s.invalid = true;
                }
                changed
            });
            if changed {
                screen.raw_set("width", size.0)?;
                screen.raw_set("height", size.1)?;
                screen.raw_set("orientation", name)?;
            }
            true.into_lua_multi(lua)
        })
    };
    add_methods(
        lua,
        &screen,
        "screen",
        &[("set_orientation", set_orientation)],
    )?;
    ctx.raw_set("screen", screen)
}
