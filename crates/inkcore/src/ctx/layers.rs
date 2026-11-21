//! `ctx.layers`: offscreen images the app draws into with its own `g` (0.1 `ctx_ui.layer_create`).
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mlua::{IntoLuaMulti, Lua, Table};

use super::image::new_image;
use super::object::{method, new_object};
use super::{arg, check_arg, nil, round_ms, soft_fail};
use crate::draw::image::Gray;
use crate::draw::{new_g, Canvas, Target};
use crate::emu::{with, Host, Shared};

const MAX_LAYER_SIDE: i64 = 2048;
const RELEASED: &str = "layer:draw: the layer was released";

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
            let canvas = Rc::new(RefCell::new(Canvas::new(w, h)));
            let g = new_g(lua, &st, Target::Layer(canvas.clone()))?;
            let id = Rc::new(Cell::new(0));
            let draw = {
                let (st, id) = (st.clone(), id.clone());
                method(move |lua, args| {
                    let a = args.into_vec();
                    check_arg("draw", 1, arg(&a, 0), "function")?;
                    let id = id.get();
                    if !with(&st, |s| s.images.contains_key(&id)) {
                        return Err(mlua::Error::runtime(RELEASED));
                    }
                    canvas.borrow_mut().begin_frame();
                    let host = Host { lua, st: &st };
                    if host
                        .protected_value(arg(&a, 0), g.clone(), "layer:draw callback")
                        .is_none()
                    {
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
                    let base = with(&st, |s| s.images.get(&id).cloned())
                        .ok_or_else(|| mlua::Error::runtime(RELEASED))?;
                    let frame = canvas.borrow().render_on(base.to_frame());
                    let img = Rc::new(Gray::from_frame(&frame));
                    with(&st, |s| s.images.insert(id, img));
                    Ok(nil())
                })
            };
            let (obj, new_id) =
                new_image(lua, &st, Gray::white(w, h), "layer", vec![("draw", draw)])?;
            id.set(new_id);
            obj.into_lua_multi(lua)
        })
    };
    ctx.raw_set("layers", new_object(lua, "layers", &[("create", create)])?)
}
