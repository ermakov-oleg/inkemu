//! `ctx.storage` and `ctx.files` over the persisted files (0.1 `_install_files`). File data stays bytes.
use mlua::{IntoLuaMulti, Lua, Table, Value};

use super::object::{method, new_object, Method};
use super::{arg, int_value, raw, soft_fail};
use crate::emu::{with, Shared};

/// A storage file name the app may use (0.1 `_valid_name`): UTF-8, at most 255 bytes, no whitespace, slashes or
/// control characters, not `.` or `..`.
pub fn valid_name(b: &[u8]) -> bool {
    let Ok(name) = std::str::from_utf8(b) else {
        return false;
    };
    !name.is_empty()
        && name != "."
        && name != ".."
        && b.len() <= 255
        && !name
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '/' | '\\' | '\0'..='\x1f' | '\x7f'))
}

/// The file name in a method's first argument, if valid.
fn target(v: &Value) -> Option<String> {
    raw(v)
        .filter(|b| valid_name(b))
        .map(|b| String::from_utf8(b).expect("checked by valid_name"))
}

fn methods(st: &Shared) -> Vec<(&'static str, Method)> {
    let write = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            let (Some(name), Some(body)) = (target(arg(&a, 0)), raw(arg(&a, 1))) else {
                return soft_fail(lua, "invalid_argument");
            };
            let stored = with(&st, |s| {
                let (max_bytes, max_files) = s.cfg.storage_quota;
                let (n, bytes) = s
                    .persisted
                    .files
                    .iter()
                    .filter(|(k, _)| **k != name)
                    .fold((0usize, 0u64), |(n, b), (_, v)| (n + 1, b + v.len() as u64));
                if body.len() as u64 + bytes > max_bytes || n + 1 > max_files {
                    return false;
                }
                if s.persisted.files.get(&name) != Some(&body) {
                    s.persisted.files.insert(name, body);
                    s.persist_changed = true;
                }
                true
            });
            if stored {
                true.into_lua_multi(lua)
            } else {
                soft_fail(lua, "no_space")
            }
        })
    };
    let read = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            let Some(name) = target(arg(&a, 0)) else {
                return soft_fail(lua, "invalid_argument");
            };
            if a.iter().skip(1).any(|v| !v.is_nil()) {
                with(&st, |s| {
                    s.warn("storage:read: offset/length ignored; the device always returns the whole file")
                });
            }
            match with(&st, |s| s.persisted.files.get(&name).cloned()) {
                Some(data) => lua.create_string(data)?.into_lua_multi(lua),
                None => soft_fail(lua, "not_found"),
            }
        })
    };
    let exists = {
        let st = st.clone();
        method(move |lua, args| {
            let name = target(args.iter().next().unwrap_or(&Value::Nil));
            let found = name.is_some_and(|n| with(&st, |s| s.persisted.files.contains_key(&n)));
            found.into_lua_multi(lua)
        })
    };
    let size = {
        let st = st.clone();
        method(move |lua, args| {
            let Some(name) = target(args.iter().next().unwrap_or(&Value::Nil)) else {
                return soft_fail(lua, "invalid_argument");
            };
            match with(&st, |s| s.persisted.files.get(&name).map(Vec::len)) {
                Some(n) => int_value(n as i64).into_lua_multi(lua),
                None => soft_fail(lua, "not_found"),
            }
        })
    };
    let remove = {
        let st = st.clone();
        method(move |lua, args| {
            let Some(name) = target(args.iter().next().unwrap_or(&Value::Nil)) else {
                return soft_fail(lua, "invalid_argument");
            };
            let removed = with(&st, |s| {
                let removed = s.persisted.files.remove(&name).is_some();
                s.persist_changed |= removed;
                removed
            });
            if removed {
                true.into_lua_multi(lua)
            } else {
                soft_fail(lua, "not_found")
            }
        })
    };
    let usage = {
        let st = st.clone();
        method(move |lua, _| {
            let (bytes, files, (max_bytes, max_files)) = with(&st, |s| {
                let f = &s.persisted.files;
                let bytes: usize = f.values().map(Vec::len).sum();
                (bytes, f.len(), s.cfg.storage_quota)
            });
            let t = lua.create_table()?;
            t.raw_set("used_bytes", int_value(bytes as i64))?;
            t.raw_set("used_files", int_value(files as i64))?;
            t.raw_set("max_bytes", int_value(max_bytes as i64))?;
            t.raw_set("max_files", int_value(max_files as i64))?;
            t.into_lua_multi(lua)
        })
    };
    let list = {
        let st = st.clone();
        method(move |lua, _| {
            let names: Vec<String> = with(&st, |s| s.persisted.files.keys().cloned().collect());
            lua.create_sequence_from(names)?.into_lua_multi(lua)
        })
    };
    vec![
        ("write", write),
        ("read", read),
        ("exists", exists),
        ("size", size),
        ("remove", remove),
        ("usage", usage),
        ("list", list),
    ]
}

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    ctx.raw_set("storage", new_object(lua, "storage", &methods(st))?)?;
    let files = new_object(lua, "files", &methods(st))?;
    let app_id = with(st, |s| s.app_id.clone());
    files.raw_set("path", format!("/AppFiles/{app_id}"))?;
    ctx.raw_set("files", files)
}
