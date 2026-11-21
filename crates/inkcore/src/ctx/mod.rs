//! The tables the app sees as `ctx` and its members.
pub(crate) mod object;
mod ui;

use std::rc::Rc;

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value};

use crate::emu::{with, Host, Shared};
use object::{method, new_object};

/// The global `ctx` with its own methods; the sub-objects are added by their own modules.
pub(crate) fn new_ctx(lua: &Lua, st: &Shared) -> mlua::Result<Table> {
    let invalidate = {
        let st = st.clone();
        method(move |_, _| {
            with(&st, |s| s.invalid = true);
            Ok(MultiValue::new())
        })
    };
    let log = {
        let st = st.clone();
        method(move |_, args| {
            let parts: Vec<String> = args.iter().map(fmt).collect();
            with(&st, |s| s.log_line(&format!("[app] {}", parts.join(" "))));
            Ok(MultiValue::new())
        })
    };
    let after = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            check_arg("after", 1, arg(&a, 0), "number")?;
            check_arg("after", 2, arg(&a, 1), "function")?;
            let ms = round_ms(arg(&a, 0))?;
            let Value::Function(f) = arg(&a, 1).clone() else {
                unreachable!()
            };
            let fire = Rc::new(move |h: &Host| {
                h.protected(&f, h.ctx(), "ctx:after callback");
            });
            let id = with(&st, |s| s.add_timer(ms, fire, 0));
            let st = st.clone();
            let cancel = method(move |_, _| {
                with(&st, |s| s.cancel_timer(id));
                Ok(MultiValue::new())
            });
            new_object(lua, "timer", &[("cancel", cancel)])?.into_lua_multi(lua)
        })
    };
    let set_tick = {
        let st = st.clone();
        method(move |_, args| {
            let a = args.into_vec();
            check_arg("set_tick", 1, arg(&a, 0), "number")?;
            let ms = round_ms(arg(&a, 0))?;
            with(&st, |s| {
                if let Some(t) = s.tick.take() {
                    s.cancel_timer(t);
                }
                if ms > 0 {
                    let tick = Rc::new(|h: &Host| {
                        h.call("on_tick", h.ctx());
                    });
                    s.tick = Some(s.add_timer(ms, tick, ms));
                }
            });
            Ok(MultiValue::new())
        })
    };
    let quit = {
        let st = st.clone();
        method(move |_, _| {
            with(&st, |s| {
                if s.locked {
                    s.log_line("ctx:quit ignored on the lock screen");
                } else {
                    s.quit = true;
                }
            });
            Ok(MultiValue::new())
        })
    };
    let ctx = new_object(
        lua,
        "ctx",
        &[
            ("invalidate", invalidate),
            ("log", log),
            ("after", after),
            ("set_tick", set_tick),
            ("quit", quit),
        ],
    )?;
    ui::install(lua, &ctx, st)?;
    Ok(ctx)
}

fn arg(a: &[Value], i: usize) -> &Value {
    a.get(i).unwrap_or(&Value::Nil)
}

/// 0.1 `lua_typename`.
fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Integer(_) | Value::Number(_) => "number",
        Value::Table(_) => "table",
        Value::String(_) => "string",
        Value::Boolean(_) => "boolean",
        Value::Nil => "nil",
        Value::Function(_) => "function",
        Value::Thread(_) => "thread",
        _ => "userdata",
    }
}

/// 0.1 `check_arg`.
fn check_arg(f: &str, i: usize, v: &Value, expected: &str) -> mlua::Result<()> {
    let got = type_name(v);
    if got == expected {
        return Ok(());
    }
    Err(mlua::Error::runtime(format!(
        "bad argument #{i} to '{f}' ({expected} expected, got {got})"
    )))
}

/// Python `round()` of a Lua number (checked by the caller) to whole milliseconds.
fn round_ms(v: &Value) -> mlua::Result<i64> {
    let n = match v {
        Value::Integer(i) => return Ok(i64::from(*i)),
        Value::Number(n) => f64::from(*n),
        _ => 0.0,
    };
    if n.is_nan() {
        return Err(mlua::Error::runtime("cannot convert float NaN to integer"));
    }
    if n.is_infinite() {
        return Err(mlua::Error::runtime(
            "cannot convert float infinity to integer",
        ));
    }
    Ok(n.round_ties_even() as i64)
}

/// 0.1 `_opt_str`: Python `str(opts[key])`, "" for nil.
fn opt_str(opts: &Table, key: &str) -> mlua::Result<String> {
    Ok(match opts.get::<Value>(key)? {
        Value::Nil => String::new(),
        Value::String(s) => s.to_string_lossy(),
        Value::Boolean(b) => if b { "True" } else { "False" }.into(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => {
            let n = f64::from(n);
            if n.fract() == 0.0 && n.abs() < 1e16 {
                format!("{n:.1}")
            } else {
                n.to_string()
            }
        }
        other => format!("<Lua {}>", type_name(&other)),
    })
}

/// 0.1 `_fmt`: how `ctx:log` prints one value.
fn fmt(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Boolean(b) => b.to_string(),
        Value::String(s) => s.to_string_lossy(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.type_name().to_string(),
    }
}
