//! `ctx.storage` and `ctx.files` (0.1 `_install_files`): two directories on firmware 26112001, each with its own
//! quota. File data stays bytes.
use std::collections::BTreeMap;

use mlua::{IntoLuaMulti, Lua, Table, Value};

use super::object::{method, new_object, Method};
use super::{arg, int_value, raw, soft_fail};
use crate::emu::{with, Shared, State};

type Dir = fn(&mut State) -> &mut BTreeMap<String, Vec<u8>>;

fn storage_dir(s: &mut State) -> &mut BTreeMap<String, Vec<u8>> {
    &mut s.persisted.files
}

fn files_dir(s: &mut State) -> &mut BTreeMap<String, Vec<u8>> {
    &mut s.persisted.app_files
}

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

/// `write` (`append` false) or `append`: the file as given, or the given data added to its end.
fn writer(st: &Shared, dir: Dir, append: bool) -> Method {
    let st = st.clone();
    method(move |lua, args| {
        let a = args.into_vec();
        let (Some(name), Some(data)) = (target(arg(&a, 0)), raw(arg(&a, 1))) else {
            return soft_fail(lua, "invalid_argument");
        };
        let stored = with(&st, |s| {
            let (max_bytes, max_files) = s.cfg.storage_quota;
            let files = dir(s);
            let body = match files.get(&name) {
                Some(old) if append => [old.as_slice(), &data].concat(),
                _ => data,
            };
            let (n, bytes) = files
                .iter()
                .filter(|(k, _)| **k != name)
                .fold((0usize, 0u64), |(n, b), (_, v)| (n + 1, b + v.len() as u64));
            if body.len() as u64 + bytes > max_bytes || n + 1 > max_files {
                return false;
            }
            if files.get(&name) != Some(&body) {
                files.insert(name, body);
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
}

/// The methods both directories have.
fn methods(st: &Shared, dir: Dir) -> Vec<(&'static str, Method)> {
    let write = writer(st, dir, false);
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
            match with(&st, |s| dir(s).get(&name).cloned()) {
                Some(data) => lua.create_string(data)?.into_lua_multi(lua),
                None => soft_fail(lua, "not_found"),
            }
        })
    };
    let exists = {
        let st = st.clone();
        method(move |lua, args| {
            let name = target(args.iter().next().unwrap_or(&Value::Nil));
            let found = name.is_some_and(|n| with(&st, |s| dir(s).contains_key(&n)));
            found.into_lua_multi(lua)
        })
    };
    let size = {
        let st = st.clone();
        method(move |lua, args| {
            let Some(name) = target(args.iter().next().unwrap_or(&Value::Nil)) else {
                return soft_fail(lua, "invalid_argument");
            };
            match with(&st, |s| dir(s).get(&name).map(Vec::len)) {
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
                let removed = dir(s).remove(&name).is_some();
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
    let list = {
        let st = st.clone();
        method(move |lua, _| {
            let names: Vec<String> = with(&st, |s| dir(s).keys().cloned().collect());
            lua.create_sequence_from(names)?.into_lua_multi(lua)
        })
    };
    vec![
        ("write", write),
        ("read", read),
        ("exists", exists),
        ("size", size),
        ("remove", remove),
        ("list", list),
    ]
}

/// `storage:usage()`; the device's `ctx.files` has none.
fn usage(st: &Shared) -> Method {
    let st = st.clone();
    method(move |lua, _| {
        let (bytes, files, (max_bytes, max_files)) = with(&st, |s| {
            let quota = s.cfg.storage_quota;
            let f = storage_dir(s);
            let bytes: usize = f.values().map(Vec::len).sum();
            (bytes, f.len(), quota)
        });
        let t = lua.create_table()?;
        t.raw_set("bytes", int_value(bytes as i64))?;
        t.raw_set("files", int_value(files as i64))?;
        t.raw_set("max_bytes", int_value(max_bytes as i64))?;
        t.raw_set("max_files", int_value(max_files as i64))?;
        t.into_lua_multi(lua)
    })
}

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let mut storage_methods = methods(st, storage_dir);
    storage_methods.push(("usage", usage(st)));
    ctx.raw_set("storage", new_object(lua, "storage", &storage_methods)?)?;
    let mut file_methods = methods(st, files_dir);
    file_methods.push(("append", writer(st, files_dir, true)));
    let files = new_object(lua, "files", &file_methods)?;
    let app_id = with(st, |s| s.app_id.clone());
    files.raw_set("path", format!("/AppFiles/{app_id}"))?;
    ctx.raw_set("files", files)
}
