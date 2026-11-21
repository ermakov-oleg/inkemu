//! The tables the app sees as `ctx` and its members.
pub(crate) mod object;

use mlua::{Lua, MultiValue, Table, Value};

use crate::emu::{with, Shared};
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
    new_object(lua, "ctx", &[("invalidate", invalidate), ("log", log)])
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
