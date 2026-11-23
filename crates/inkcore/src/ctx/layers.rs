//! `ctx.layers`: offscreen images the app draws into with its own `g` (0.1 `ctx_ui.layer_create`).
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mlua::{IntoLuaMulti, Lua, Table};

use super::image::{new_image, Kind};
use super::object::{method, new_object};
use super::{arg, check_arg, nil, round_ms, soft_fail};
use crate::draw::image::Gray;
use crate::draw::{new_g, Canvas, Target};
use crate::emu::{with, Host, Shared};

const MAX_LAYER_SIDE: i64 = 2048;

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let create = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            check_arg("create", 1, arg(&a, 0), "number")?;
            check_arg("create", 2, arg(&a, 1), "number")?;
            let (w, h) = (round_ms(arg(&a, 0))?, round_ms(arg(&a, 1))?);
            let side = 1..=MAX_LAYER_SIDE;
            if !side.contains(&w) || !side.contains(&h) {
                return soft_fail(lua, "invalid_argument");
            }
            let (w, h) = (w as u32, h as u32);
            let canvas = Rc::new(RefCell::new(Canvas::new(
                w,
                h,
                with(&st, |s| s.fonts.clone()),
            )));
            let id = Rc::new(Cell::new(0));
            let target = Target::Layer {
                canvas: canvas.clone(),
                id: id.clone(),
            };
            let g = new_g(lua, &st, target)?;
            let draw = {
                let (st, id) = (st.clone(), id.clone());
                method(move |lua, args| {
                    let a = args.into_vec();
                    check_arg("draw", 1, arg(&a, 0), "function")?;
                    let id = id.get();
                    if !with(&st, |s| s.images.contains_key(&id)) {
                        return soft_fail(lua, "invalid_handle");
                    }
                    let was = {
                        let mut c = canvas.borrow_mut();
                        c.begin_frame();
                        c.set_active(true)
                    };
                    let host = Host { lua, st: &st };
                    let done = host.protected_value(arg(&a, 0), g.clone(), "layer:draw callback");
                    canvas.borrow_mut().set_active(was);
                    if done.is_none() {
                        return Ok(nil());
                    }
                    let (warnings, bad) = {
                        let mut c = canvas.borrow_mut();
                        c.end_frame();
                        (c.take_warnings(), c.take_bad())
                    };
                    with(&st, |s| warnings.iter().for_each(|w| s.warn(w)));
                    if let Some(bad) = bad {
                        return Err(mlua::Error::runtime(format!(
                            "g: bad values in layer:draw: {bad}"
                        )));
                    }
                    // released inside its own callback: nothing is left to draw into
                    let Some(base) = with(&st, |s| s.images.get(&id).cloned()) else {
                        return soft_fail(lua, "invalid_handle");
                    };
                    let frame = canvas.borrow().render_on(base.to_frame());
                    let img = Rc::new(Gray::from_frame(&frame));
                    with(&st, |s| s.images.insert(id, img));
                    Ok(nil())
                })
            };
            let (obj, new_id) = new_image(
                lua,
                &st,
                Gray::white(w, h),
                Kind::Runtime,
                "layer",
                vec![("draw", draw)],
            )?;
            id.set(new_id);
            obj.into_lua_multi(lua)
        })
    };
    ctx.raw_set("layers", new_object(lua, "layers", &[("create", create)])?)
}
