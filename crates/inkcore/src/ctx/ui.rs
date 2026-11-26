//! `ctx.dialog` and `ctx.input`: dialogs, text input, toasts (0.1 `ctx_ui.install`, dialog part).
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value};

use super::object::{method, new_object};
use super::{arg, check_arg, nil, opt_str, soft_fail, type_name};
use crate::dialog::Handler;
use crate::emu::{with, Host, Shared};
use crate::input::KEYBOARD_H;

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
            if with(&st, |s| s.lock.ctx.is_some()) {
                return soft_fail(lua, "inactive");
            }
            if busy(&Host { lua, st: &st }) {
                return soft_fail(lua, "busy");
            }
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
                type_text: Some(done.clone()),
                submit: Some(done),
                ok_keeps_open: false,
                rect: None,
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
            let h = Host { lua, st: &st };
            let screen = with(&st, |s| (s.canvas.w, s.canvas.h));
            let Some(o) = edit_opts(opts, screen)? else {
                return soft_fail(lua, "invalid_argument");
            };
            if with(&st, |s| s.lock.ctx.is_some()) {
                return soft_fail(lua, "inactive");
            }
            if busy(&h) {
                return soft_fail(lua, "busy");
            }
            let ed = Rc::new(Edit {
                text: RefCell::new(o.text),
                id: Cell::new(0),
                max_chars: o.max_chars,
                multiline: o.multiline,
                newline: o.newline,
                on_change: o.on_change,
                on_end: o.on_end,
            });
            let set_text = {
                let (st, ed) = (st.clone(), ed.clone());
                method(move |lua, args| {
                    let a = args.into_vec();
                    check_arg("set_text", 1, arg(&a, 0), "string")?;
                    let Value::String(s) = arg(&a, 0) else {
                        unreachable!()
                    };
                    let h = Host { lua, st: &st };
                    if !ed.active(&h) {
                        return soft_fail(lua, "inactive");
                    }
                    let text = String::from_utf8(s.as_bytes().to_vec()).ok();
                    match text.filter(|t| text_ok(t, ed.max_chars, ed.multiline)) {
                        Some(t) => {
                            ed.sync(&h, t);
                            true.into_lua_multi(lua)
                        }
                        None => soft_fail(lua, "invalid_argument"),
                    }
                })
            };
            let text = {
                let (st, ed) = (st.clone(), ed.clone());
                method(move |lua, _| {
                    if !ed.active(&Host { lua, st: &st }) {
                        return Ok(nil());
                    }
                    let text = ed.text.borrow().clone();
                    text.into_lua_multi(lua)
                })
            };
            let close = {
                let (st, ed) = (st.clone(), ed.clone());
                method(move |lua, _| {
                    let h = Host { lua, st: &st };
                    if !ed.active(&h) {
                        return Ok(nil());
                    }
                    h.clear_pending();
                    let text = ed.text.borrow().clone();
                    text.into_lua_multi(lua)
                })
            };
            let obj = new_object(
                lua,
                "edit",
                &[("set_text", set_text), ("text", text), ("close", close)],
            )?;
            let (on_answer, on_type, on_submit) = (ed.clone(), ed.clone(), ed.clone());
            let handler = Handler {
                answer: Rc::new(move |h, ok| match ok {
                    true if on_answer.newline => {
                        on_answer.insert(h, '\n');
                    }
                    true => on_answer.finish(h, "enter"),
                    false => on_answer.finish(h, "back"),
                }),
                type_text: Some(Rc::new(move |h, s| on_type.type_text(h, s))),
                submit: Some(Rc::new(move |h, s| on_submit.submit(h, s))),
                ok_keeps_open: ed.newline,
                rect: o.rect,
            };
            let text = ed.text.borrow().clone();
            ed.id
                .set(h.set_pending("edit", opt_str(opts, "title")?, text, handler));
            if let Some((x, y, w, rh)) = o.rect {
                let msg = format!("edit rect {x},{y} {w}x{rh}: its text area is not drawn");
                with(&st, |s| s.log_line(&msg));
            }
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
            if with(&st, |s| s.lock.ctx.is_some()) {
                return soft_fail(lua, "inactive");
            }
            let h = Host { lua, st: &st };
            if h.pending_kind() == Some("edit") {
                return soft_fail(lua, "busy");
            }
            let handler = Handler {
                answer: Rc::new(move |h, ok| {
                    h.protected(&f, (h.ctx(), ok), "ctx.dialog:confirm callback");
                }),
                type_text: None,
                submit: None,
                ok_keeps_open: false,
                rect: None,
            };
            let (title, text) = (opt_str(opts, "title")?, opt_str(opts, "text")?);
            h.set_pending("confirm", title, text, handler);
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
            if with(&st, |s| s.lock.ctx.is_some()) {
                return soft_fail(lua, "inactive");
            }
            Host { lua, st: &st }.show_toast(&s.to_string_lossy());
            Ok(MultiValue::new())
        })
    };
    let input = new_object(lua, "input", &[("text", text), ("edit", edit)])?;
    input.raw_set("keyboard_h", KEYBOARD_H)?;
    input.raw_set("pointer", true)?;
    ctx.raw_set("input", input)?;
    ctx.raw_set(
        "dialog",
        new_object(lua, "dialog", &[("confirm", confirm), ("toast", toast)])?,
    )?;
    Ok(())
}

/// An open text input, docked input or confirm makes another of them `busy` (§4).
fn busy(h: &Host) -> bool {
    h.pending_kind()
        .is_some_and(|k| matches!(k, "text" | "edit" | "confirm"))
}

/// The `ctx.input:edit` options the emulation uses, after the §4 checks.
struct EditOpts {
    text: String,
    max_chars: usize,
    multiline: bool,
    newline: bool,
    rect: Option<(i64, i64, i64, i64)>,
    on_change: Value,
    on_end: Value,
}

/// An integer, or a float with an integral value in the device's 32-bit range.
fn int(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(i64::from(*i)),
        Value::Number(n) if n.fract() == 0.0 && n.abs() <= i32::MAX as f32 => Some(*n as i64),
        _ => None,
    }
}

/// A string option as UTF-8; None when it is not valid UTF-8. The type is checked by the caller.
fn utf8(v: &Value) -> Option<Option<String>> {
    match v {
        Value::String(s) => String::from_utf8(s.as_bytes().to_vec()).ok().map(Some),
        _ => Some(None),
    }
}

fn text_ok(text: &str, max_chars: usize, multiline: bool) -> bool {
    text.chars().count() <= max_chars && (multiline || !text.contains('\n'))
}

/// A Lua error for option `key` of `ctx.input:edit` that is not of type `expected` (nil passes when `nil_ok`).
fn opt_type(key: &str, v: &Value, expected: &str, nil_ok: bool) -> mlua::Result<()> {
    let got = type_name(v);
    if got == expected || (nil_ok && v.is_nil()) {
        return Ok(());
    }
    Err(mlua::Error::runtime(format!(
        "bad option '{key}' to 'edit' ({expected} expected, got {got})"
    )))
}

/// The options checked against the §4 table: a value of the wrong type raises (§0 rule 3), and every type is
/// checked before any range, so the outcome does not depend on the order; Ok(None) when a value breaks its rule.
fn edit_opts(opts: &Table, (sw, sh): (u32, u32)) -> mlua::Result<Option<EditOpts>> {
    let get = |k: &str| opts.get::<Value>(k);
    let (max_chars, multiline) = (get("max_chars")?, get("multiline")?);
    let (text, placeholder) = (get("text")?, get("placeholder")?);
    let (kind, enter, rect) = (get("kind")?, get("enter")?, get("rect")?);
    let (on_change, on_end) = (get("on_change")?, get("on_end")?);
    opt_type("max_chars", &max_chars, "number", true)?;
    opt_type("multiline", &multiline, "boolean", true)?;
    for (k, v) in [
        ("text", &text),
        ("placeholder", &placeholder),
        ("kind", &kind),
        ("enter", &enter),
    ] {
        opt_type(k, v, "string", true)?;
    }
    opt_type("rect", &rect, "table", true)?;
    let mut r = [0; 4];
    if let Value::Table(t) = &rect {
        for (n, k) in r.iter_mut().zip(["x", "y", "w", "h"]) {
            let v = t.get::<Value>(k)?;
            opt_type(&format!("rect.{k}"), &v, "number", false)?;
            // a fraction is a number out of range
            *n = int(&v).unwrap_or(-1);
        }
    }
    opt_type("on_change", &on_change, "function", true)?;
    opt_type("on_end", &on_end, "function", true)?;

    let max_chars = match max_chars {
        Value::Nil => 128,
        v => match int(&v) {
            Some(n @ 1..=512) => n as usize,
            _ => return Ok(None),
        },
    };
    let multiline = matches!(multiline, Value::Boolean(true));
    let (Some(text), Some(placeholder)) = (utf8(&text), utf8(&placeholder)) else {
        return Ok(None);
    };
    let text = text.unwrap_or_default();
    let (Some(kind), Some(enter)) = (utf8(&kind), utf8(&enter)) else {
        return Ok(None);
    };
    let kind_ok = matches!(
        kind.as_deref(),
        None | Some("text" | "number" | "secret" | "letters")
    );
    let enter = enter.as_deref();
    let enter_ok = match enter {
        None | Some("done" | "save" | "next" | "search") => true,
        Some("newline") => multiline,
        Some(_) => false,
    };
    let fits = text_ok(&text, max_chars, multiline);
    if !(kind_ok && enter_ok && fits && placeholder.map_or(0, |p| p.len()) <= 128) {
        return Ok(None);
    }
    let rect = match rect {
        Value::Nil => None,
        _ => {
            let [x, y, w, h] = r;
            let on_screen = x >= 0 && y >= 0 && x + w <= i64::from(sw) && y + h <= i64::from(sh);
            if w < 1 || h < 1 || !on_screen {
                return Ok(None);
            }
            Some((x, y, w, h))
        }
    };
    Ok(Some(EditOpts {
        text,
        max_chars,
        multiline,
        newline: enter == Some("newline"),
        rect,
        on_change,
        on_end,
    }))
}

/// One `ctx.input:edit` session: the committed text and the dialog it belongs to.
struct Edit {
    text: RefCell<String>,
    id: Cell<u64>,
    max_chars: usize,
    multiline: bool,
    /// `enter = "newline"`: the confirm key inserts a line break instead of ending editing.
    newline: bool,
    on_change: Value,
    on_end: Value,
}

impl Edit {
    fn active(&self, h: &Host) -> bool {
        h.is_pending(self.id.get())
    }

    fn sync(&self, h: &Host, text: String) {
        if self.active(h) {
            with(h.st, |s| {
                if let Some(p) = s.pending.as_mut() {
                    p.text = text.clone();
                }
            });
        }
        *self.text.borrow_mut() = text;
    }

    fn finish(&self, h: &Host, how: &str) {
        if self.active(h) {
            h.clear_pending();
        }
        if !self.on_end.is_nil() {
            let text = self.text.borrow().clone();
            h.protected_value(&self.on_end, (h.ctx(), text, how), "ctx.input:edit on_end");
        }
    }

    /// Types one character; false once editing has ended. A newline is the confirm key.
    fn insert(&self, h: &Host, ch: char) -> bool {
        if ch == '\n' && !self.newline {
            self.finish(h, "enter");
            return false;
        }
        let next = {
            let text = self.text.borrow();
            (text.chars().count() < self.max_chars).then(|| format!("{text}{ch}"))
        };
        let Some(next) = next else {
            return true;
        };
        self.sync(h, next);
        if !self.on_change.is_nil() {
            let text = self.text.borrow().clone();
            h.protected_value(&self.on_change, (h.ctx(), text), "ctx.input:edit on_change");
        }
        // closed by on_change, or the app failed
        self.active(h)
    }

    /// Replaces the text with `s` (cut to the text rules), then presses the confirm key.
    fn submit(&self, h: &Host, s: &str) {
        let s = if self.multiline {
            s
        } else {
            s.split('\n').next().unwrap_or_default()
        };
        let next: String = s.chars().take(self.max_chars).collect();
        if *self.text.borrow() != next {
            self.sync(h, next.clone());
            if !self.on_change.is_nil() {
                h.protected_value(&self.on_change, (h.ctx(), next), "ctx.input:edit on_change");
            }
            if !self.active(h) {
                return;
            }
        }
        // a newline edit stays open: only Back or close() end it (§4)
        if !self.newline {
            self.finish(h, "enter");
        }
    }

    /// Types `s`, then presses the confirm key unless it inserts line breaks.
    fn type_text(&self, h: &Host, s: &str) {
        if s.chars().all(|ch| self.insert(h, ch)) && !self.newline {
            self.finish(h, "enter");
        }
    }
}
