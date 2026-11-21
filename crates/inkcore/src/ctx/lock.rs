//! `ctx.lock` inside the lock screen state and `ctx.lockscreen` in the app when the manifest has a `lockscreen`
//! block (0.1 `lockscreen.py`).
use mlua::{IntoLuaMulti, Lua, Table, Value};
use serde_json::{Map, Value as J};

use super::convert::dump;
use super::object::{method, new_object, Method};
use super::{arg, check_arg, int_value, nil, soft_fail};
use crate::emu::{with, Shared, State};
use crate::lockscreen::{self, Num};

pub(super) fn install(
    lua: &Lua,
    ctx: &Table,
    st: &Shared,
    lockscreen_api: bool,
) -> mlua::Result<()> {
    let lock = with(st, |s| {
        let c = s.lock.ctx.as_ref()?;
        Some((c.reason, c.net, c.elapsed_sec, s.lock.interval))
    });
    match lock {
        Some(fields) => install_lock(lua, ctx, st, fields),
        None if lockscreen_api => install_api(lua, ctx, st),
        None => Ok(()),
    }
}

fn num_value(n: Num) -> Value {
    match n {
        Num::Int(i) => int_value(i),
        Num::Float(f) => Value::Number(f as f32),
    }
}

fn install_lock(
    lua: &Lua,
    ctx: &Table,
    st: &Shared,
    (reason, net, elapsed_sec, interval): (&str, bool, i64, Num),
) -> mlua::Result<()> {
    let done = {
        let st = st.clone();
        method(move |_, _| {
            with(&st, |s| {
                if let Some(c) = &mut s.lock.ctx {
                    c.done = true;
                }
            });
            Ok(nil())
        })
    };
    let set_interval = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            let sec = arg(&a, 0);
            check_arg("set_interval", 1, sec, "number")?;
            let n = match sec {
                Value::Integer(i) => Num::Int(i64::from(*i)),
                Value::Number(f) => Num::Float(f64::from(*f)),
                _ => unreachable!(),
            };
            if !n.positive() {
                return soft_fail(lua, "invalid_argument");
            }
            let ctx = with(&st, |s| {
                s.lock.interval = n;
                lockscreen::schedule(s);
                s.ctx.clone()
            });
            if let Some(ctx) = ctx {
                ctx.get::<Table>("lock")?.set("interval_sec", sec.clone())?;
            }
            true.into_lua_multi(lua)
        })
    };
    let obj = new_object(
        lua,
        "lock",
        &[("done", done), ("set_interval", set_interval)],
    )?;
    obj.raw_set("reason", reason)?;
    obj.raw_set("net", net)?;
    obj.raw_set("interval_sec", num_value(interval))?;
    obj.raw_set("elapsed_sec", int_value(elapsed_sec))?;
    ctx.raw_set("lock", obj)
}

/// `lock.json` as a JSON object; anything else reads as empty.
fn load(s: &State) -> Map<String, J> {
    match s
        .persisted
        .lock_json
        .as_deref()
        .and_then(|b| serde_json::from_slice(b).ok())
    {
        Some(J::Object(m)) => m,
        _ => Map::new(),
    }
}

/// Python `json.dumps(..., sort_keys=True)`, which escapes non-ASCII text.
fn dump_ascii(v: &J) -> String {
    let mut out = String::new();
    for c in dump(v, false).chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

fn install_api(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let setter = |key: &'static str, value: bool, what: &'static str| -> Method {
        let st = st.clone();
        method(move |lua, _| {
            with(&st, |s| {
                let mut data = load(s);
                data.insert(key.into(), J::Bool(value));
                s.persisted.lock_json = Some(dump_ascii(&J::Object(data)).into_bytes());
                s.persist_changed = true;
                let confirmed = if value { " (auto-confirmed)" } else { "" };
                s.log_line(&format!("lockscreen: {what}{confirmed}"));
            });
            true.into_lua_multi(lua)
        })
    };
    let getter = |key: &'static str| -> Method {
        let st = st.clone();
        method(move |lua, _| {
            let on = with(&st, |s| load(s).get(key).is_some_and(lockscreen::truthy));
            on.into_lua_multi(lua)
        })
    };
    let obj = new_object(
        lua,
        "lockscreen",
        &[
            ("set_default", setter("is_default", true, "set_default")),
            (
                "clear_default",
                setter("is_default", false, "clear_default"),
            ),
            ("is_default", getter("is_default")),
            (
                "set_wallpaper",
                setter("is_wallpaper", true, "set_wallpaper"),
            ),
            (
                "clear_wallpaper",
                setter("is_wallpaper", false, "clear_wallpaper"),
            ),
            ("is_wallpaper", getter("is_wallpaper")),
        ],
    )?;
    ctx.raw_set("lockscreen", obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_json_as_python_dumps() {
        let v: J =
            serde_json::from_str(r#"{"is_wallpaper": false, "é": [1, 2.5], "is_default": true}"#)
                .unwrap();
        assert_eq!(
            dump_ascii(&v),
            r#"{"is_default": true, "is_wallpaper": false, "\u00e9": [1, 2.5]}"#
        );
    }
}
