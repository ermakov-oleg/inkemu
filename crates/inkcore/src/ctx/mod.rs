//! The tables the app sees as `ctx` and its members.
mod basic;
mod clock;
pub(crate) mod convert;
mod device;
pub(crate) mod files;
mod fonts;
mod image;
mod layers;
pub(crate) mod object;
mod package;
mod reader;
mod screen;
mod stubs;
mod ui;
mod utils;

pub(crate) use basic::save_state;

use std::rc::Rc;

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value};

use crate::app::AppFiles;
use crate::emu::{with, Host, Shared};
use crate::log::{py_float, widen};
use object::{method, new_object};

/// The global `ctx` with its own methods; the sub-objects are added by their own modules.
pub(crate) fn new_ctx(
    lua: &Lua,
    st: &Shared,
    app: &Rc<AppFiles>,
    reader: bool,
) -> mlua::Result<Table> {
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
    basic::install(lua, &ctx, st)?;
    clock::install(lua, &ctx, st)?;
    device::install(lua, &ctx, st)?;
    fonts::install(lua, &ctx)?;
    files::install(lua, &ctx, st)?;
    package::install(lua, &ctx, st, app)?;
    utils::install(lua, &ctx, st)?;
    screen::install(lua, &ctx, st)?;
    stubs::install(lua, &ctx, st)?;
    if reader {
        reader::install(lua, &ctx, st)?;
    }
    ui::install(lua, &ctx, st)?;
    image::install(lua, &ctx, st)?;
    layers::install(lua, &ctx, st)?;
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
    py_integral(v, f64::round_ties_even)
}

/// Python `int()` of a Lua number (checked by the caller).
fn py_int(v: &Value) -> mlua::Result<i64> {
    py_integral(v, f64::trunc)
}

fn py_integral(v: &Value, f: fn(f64) -> f64) -> mlua::Result<i64> {
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
    Ok(f(n) as i64)
}

/// A Lua integer when it fits the device's 32 bits, else a float.
fn int_value(n: i64) -> Value {
    i32::try_from(n).map_or(Value::Number(n as f32), Value::Integer)
}

/// What a 0.1 method returning Python `None` gave Lua: one nil.
fn nil() -> MultiValue {
    MultiValue::from_vec(vec![Value::Nil])
}

/// `(nil, reason)`, the device's soft failure.
fn soft_fail(lua: &Lua, reason: &str) -> mlua::Result<MultiValue> {
    (Value::Nil, reason).into_lua_multi(lua)
}

/// 0.1 `_opt_str`: Python `str(opts[key])`, "" for nil.
fn opt_str(opts: &Table, key: &str) -> mlua::Result<String> {
    Ok(match opts.get::<Value>(key)? {
        Value::Nil => String::new(),
        Value::String(s) => s.to_string_lossy(),
        Value::Boolean(b) => if b { "True" } else { "False" }.into(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => py_float(widen(n)),
        other => format!("<Lua {}>", type_name(&other)),
    })
}

/// 0.1 `raw`: a string's bytes; a number as `ctx:log` prints it; anything else None.
fn raw(v: &Value) -> Option<Vec<u8>> {
    match v {
        Value::String(s) => Some(s.as_bytes().to_vec()),
        Value::Integer(_) | Value::Number(_) => Some(fmt(v).into_bytes()),
        _ => None,
    }
}

/// 0.1 `_bytes_arg`: [`raw`], or an error naming the argument.
fn bytes_arg(f: &str, i: usize, v: &Value) -> mlua::Result<Vec<u8>> {
    raw(v).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "bad argument #{i} to '{f}' (string expected, got {})",
            type_name(v)
        ))
    })
}

/// 0.1 `_fmt`: how `ctx:log` prints one value. A float prints as Python's `repr` of its shortest f32 digits.
fn fmt(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Boolean(b) => b.to_string(),
        Value::String(s) => s.to_string_lossy(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => py_float(widen(*n)),
        other => type_name(other).to_string(),
    }
}
