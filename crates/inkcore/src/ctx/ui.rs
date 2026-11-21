//! `ctx.dialog` and `ctx.input`: dialogs, text input, toasts (0.1 `ctx_ui.install`, dialog part).
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value};

use super::object::{method, new_object};
use super::{arg, check_arg, opt_str};
use crate::dialog::Handler;
use crate::emu::{Host, Shared};

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let text = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            check_arg("text", 1, arg(&a, 0), "table")?;
            check_arg("text", 2, arg(&a, 1), "function")?;
            let (Value::Table(opts), Value::Function(f)) = (arg(&a, 0), arg(&a, 1).clone()) else {
                unreachable!()
            };
            let default = opt_str(opts, "text")?;
            let done = Rc::new(move |h: &Host, text: &str| {
                h.clear_pending();
                h.protected(&f, (h.ctx(), text), "ctx.input:text callback");
            });
            let ok_done = done.clone();
            let ok_text = default.clone();
            let handler = Handler {
                answer: Rc::new(move |h, ok| {
                    if ok {
                        ok_done(h, &ok_text);
                    }
                }),
                type_text: Some(done),
            };
            Host { lua, st: &st }.set_pending("text", opt_str(opts, "title")?, default, handler);
            Ok(MultiValue::new())
        })
    };
    let edit = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            check_arg("edit", 1, arg(&a, 0), "table")?;
            let Value::Table(opts) = arg(&a, 0) else {
                unreachable!()
            };
            let ed = Rc::new(Edit {
                text: RefCell::new(opt_str(opts, "text")?),
                obj: OnceCell::new(),
                id: Cell::new(0),
                on_change: opts.get("on_change")?,
                on_end: opts.get("on_end")?,
            });
            let set_text = {
                let (st, ed) = (st.clone(), ed.clone());
                method(move |lua, args| {
                    let a = args.into_vec();
                    check_arg("set_text", 1, arg(&a, 0), "string")?;
                    let Value::String(s) = arg(&a, 0) else {
                        unreachable!()
                    };
                    ed.sync(&Host { lua, st: &st }, s.to_string_lossy());
                    Ok(MultiValue::new())
                })
            };
            let close = {
                let (st, ed) = (st.clone(), ed.clone());
                method(move |lua, _| {
                    let h = Host { lua, st: &st };
                    if h.is_pending(ed.id.get()) {
                        h.clear_pending();
                    }
                    Ok(MultiValue::new())
                })
            };
            let obj = new_object(lua, "edit", &[("set_text", set_text), ("close", close)])?;
            obj.raw_set("text", ed.text.borrow().as_str())?;
            let _ = ed.obj.set(obj.clone());
            let (on_answer, on_type) = (ed.clone(), ed.clone());
            let handler = Handler {
                answer: Rc::new(move |h, ok| {
                    on_answer.finish(h, if ok { "enter" } else { "cancel" })
                }),
                type_text: Some(Rc::new(move |h, s| on_type.type_text(h, s))),
            };
            let text = ed.text.borrow().clone();
            let id =
                Host { lua, st: &st }.set_pending("edit", opt_str(opts, "title")?, text, handler);
            ed.id.set(id);
            obj.into_lua_multi(lua)
        })
    };
    let confirm = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            check_arg("confirm", 1, arg(&a, 0), "table")?;
            check_arg("confirm", 2, arg(&a, 1), "function")?;
            let (Value::Table(opts), Value::Function(f)) = (arg(&a, 0), arg(&a, 1).clone()) else {
                unreachable!()
            };
            let handler = Handler {
                answer: Rc::new(move |h, ok| {
                    h.protected(&f, (h.ctx(), ok), "ctx.dialog:confirm callback");
                }),
                type_text: None,
            };
            let (title, text) = (opt_str(opts, "title")?, opt_str(opts, "text")?);
            Host { lua, st: &st }.set_pending("confirm", title, text, handler);
            Ok(MultiValue::new())
        })
    };
    let toast = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            check_arg("toast", 1, arg(&a, 0), "string")?;
            let Value::String(s) = arg(&a, 0) else {
                unreachable!()
            };
            Host { lua, st: &st }.show_toast(&s.to_string_lossy());
            Ok(MultiValue::new())
        })
    };
    ctx.raw_set(
        "input",
        new_object(lua, "input", &[("text", text), ("edit", edit)])?,
    )?;
    ctx.raw_set(
        "dialog",
        new_object(lua, "dialog", &[("confirm", confirm), ("toast", toast)])?,
    )?;
    Ok(())
}

/// One `ctx.input:edit` session: the text so far and the dialog it belongs to.
struct Edit {
    text: RefCell<String>,
    obj: OnceCell<Table>,
    id: Cell<u64>,
    on_change: Value,
    on_end: Value,
}

impl Edit {
    fn sync(&self, h: &Host, text: String) {
        if let Some(obj) = self.obj.get() {
            let _ = obj.raw_set("text", text.as_str());
        }
        if h.is_pending(self.id.get()) {
            crate::emu::with(h.st, |s| {
                if let Some(p) = s.pending.as_mut() {
                    p.text = text.clone();
                }
            });
        }
        *self.text.borrow_mut() = text;
    }

    fn finish(&self, h: &Host, reason: &str) {
        if h.is_pending(self.id.get()) {
            h.clear_pending();
        }
        if !self.on_end.is_nil() {
            let text = self.text.borrow().clone();
            h.protected_value(
                &self.on_end,
                (h.ctx(), text, reason),
                "ctx.input:edit on_end",
            );
        }
    }

    fn type_text(&self, h: &Host, s: &str) {
        for ch in s.chars() {
            let next = format!("{}{ch}", self.text.borrow());
            self.sync(h, next);
            if !self.on_change.is_nil() {
                let text = self.text.borrow().clone();
                h.protected_value(&self.on_change, (h.ctx(), text), "ctx.input:edit on_change");
            }
            // closed by on_change, or the app failed
            if !h.is_pending(self.id.get()) {
                return;
            }
        }
        self.finish(h, "enter");
    }
}
