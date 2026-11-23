//! `ctx.package`: read-only files under the app's `data/` folder (0.1 `_install_package`).
use std::cell::Cell;
use std::rc::Rc;

use mlua::{IntoLuaMulti, Lua, Table, Value};

use super::image::{decode_image, Kind};
use super::object::{method, new_object};
use super::{arg, int_value, py_int, raw, soft_fail};
use crate::app::AppFiles;
use crate::emu::Shared;

/// The app-folder path of `data/<name>` (0.1 resolved it and required it to stay inside `data/`), or the reason.
fn find(files: &AppFiles, name: &Value) -> Result<String, &'static str> {
    let b = raw(name).filter(|b| !b.is_empty() && !b.contains(&0));
    let name = b
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .filter(|n| !n.starts_with('/'))
        .ok_or("invalid_argument")?;
    let mut parts = vec!["data"];
    for p in name.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or("invalid_argument")?;
            }
            p => parts.push(p),
        }
    }
    if parts.first() != Some(&"data") {
        return Err("invalid_argument");
    }
    let path = parts.join("/");
    files.get(&path).map(|_| path).ok_or("not_found")
}

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared, app: &Rc<AppFiles>) -> mlua::Result<()> {
    // the bytes of a found file, or the soft failure to return
    let read_file = {
        let app = app.clone();
        move |a: &[Value]| find(&app, arg(a, 0)).map(|p| app.get(&p).unwrap_or_default().to_vec())
    };
    let read = {
        let read_file = read_file.clone();
        method(move |lua, args| match read_file(&args.into_vec()) {
            Ok(data) => lua.create_string(data)?.into_lua_multi(lua),
            Err(why) => soft_fail(lua, why),
        })
    };
    let open_text = {
        let read_file = read_file.clone();
        method(move |lua, args| match read_file(&args.into_vec()) {
            Ok(data) => text_reader(lua, data)?.into_lua_multi(lua),
            Err(why) => soft_fail(lua, why),
        })
    };
    let image = {
        let (read_file, st) = (read_file.clone(), st.clone());
        method(move |lua, args| match read_file(&args.into_vec()) {
            Ok(data) => decode_image(lua, &st, &data, Kind::Package),
            Err(why) => soft_fail(lua, why),
        })
    };
    let exists = {
        let app = app.clone();
        method(move |lua, args| {
            find(&app, arg(&args.into_vec(), 0))
                .is_ok()
                .into_lua_multi(lua)
        })
    };
    let size = method(move |lua, args| match read_file(&args.into_vec()) {
        Ok(data) => int_value(data.len() as i64).into_lua_multi(lua),
        Err(why) => soft_fail(lua, why),
    });
    let cache_stats = method(|lua, _| lua.create_table()?.into_lua_multi(lua));
    let package = new_object(
        lua,
        "package",
        &[
            ("cache_stats", cache_stats),
            ("read", read),
            ("open_text", open_text),
            ("image", image),
            ("exists", exists),
            ("size", size),
        ],
    )?;
    ctx.raw_set("package", package)
}

/// 0.1 `open_text`: a `BytesIO` with `read_line` (without the `\n`; nil at the end), `seek`, `tell`, `close`.
fn text_reader(lua: &Lua, data: Vec<u8>) -> mlua::Result<Table> {
    let data = Rc::new(data);
    let pos = Rc::new(Cell::new(0usize));
    let read_line = {
        let (data, pos) = (data.clone(), pos.clone());
        method(move |lua, _| {
            let rest = data.get(pos.get()..).unwrap_or_default();
            if rest.is_empty() {
                return Value::Nil.into_lua_multi(lua);
            }
            let (line, used) = match rest.iter().position(|&b| b == b'\n') {
                Some(i) => (&rest[..i], i + 1),
                None => (rest, rest.len()),
            };
            pos.set(pos.get() + used);
            lua.create_string(line)?.into_lua_multi(lua)
        })
    };
    let seek = {
        let pos = pos.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            let n = match arg(&a, 0) {
                Value::Nil => 0,
                v @ (Value::Integer(_) | Value::Number(_)) => py_int(v)?,
                _ => {
                    return Err(mlua::Error::runtime(
                        "bad argument #1 to 'seek' (number expected)",
                    ))
                }
            };
            if n < 0 {
                return Err(mlua::Error::runtime(format!("negative seek value {n}")));
            }
            pos.set(n as usize);
            true.into_lua_multi(lua)
        })
    };
    let tell = method(move |lua, _| int_value(pos.get() as i64).into_lua_multi(lua));
    let close = method(|lua, _| true.into_lua_multi(lua));
    new_object(
        lua,
        "text",
        &[
            ("read_line", read_line),
            ("seek", seek),
            ("tell", tell),
            ("close", close),
        ],
    )
}
