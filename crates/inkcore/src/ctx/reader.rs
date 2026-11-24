//! `ctx.reader`, only with the "reader" permission: reading stats from the host's `reader.json` (0.1
//! `_install_reader`).
use mlua::{IntoLuaMulti, Lua, Table, Value};
use serde_json::{json, Value as J};

use super::convert::{big_int_warning, json_to_lua};
use super::object::{method, new_object, Method};
use crate::emu::{with, Shared};
use crate::lua::first_in_callback;

/// One key of `reader.json`, or `default` when the file is missing, unreadable or not an object.
fn part(st: &Shared, key: &'static str, default: J) -> Method {
    let st = st.clone();
    method(move |lua, _| {
        let raw = with(&st, |s| s.cfg.reader_json.clone());
        let data = raw.and_then(|b| match serde_json::from_slice::<J>(&b) {
            Ok(J::Object(m)) => Some(m),
            _ => None,
        });
        let v = data
            .and_then(|mut m| m.remove(key))
            .unwrap_or_else(|| default.clone());
        let mut big = None;
        let v = json_to_lua(lua, &v, None, &mut |n| {
            big.get_or_insert_with(|| big_int_warning(n));
        })?;
        if let Some(w) = big.filter(|_| first_in_callback(lua, "json-big-int")) {
            with(&st, |s| s.warn(&w));
        }
        Value::into_lua_multi(v, lua)
    })
}

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let reader = new_object(
        lua,
        "reader",
        &[
            (
                "summary",
                part(
                    st,
                    "summary",
                    json!({"streak": 0, "books": 0, "seconds": 0}),
                ),
            ),
            ("books", part(st, "books", json!([]))),
            ("all_books", part(st, "all_books", json!([]))),
            ("current", part(st, "current", J::Null)),
        ],
    )?;
    ctx.raw_set("reader", reader)
}
