//! `ctx.storage` and `ctx.files` (0.1 `_install_files`): two directories on firmware 26112001, each with its own
//! quota. File data stays bytes.
use std::collections::BTreeMap;

use mlua::{IntoLuaMulti, Lua, Table, Value};

use super::object::{method, new_object, Method};
use super::{arg, int_value, raw, soft_fail};
use crate::config::STORAGE_WRITE_MAX;
use crate::emu::{with, Shared, State};

/// The two directories; each has its own name rule and quota.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dir {
    Storage,
    App,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Write,
    Append,
}

/// An `append` may not grow an app file past this.
const APP_FILE_MAX: u64 = 2_147_483_647;
/// `files:read`: the default and the largest `len`.
const READ_DEFAULT: i64 = 65_536;
const READ_MAX: i64 = 65_536;

type Files = BTreeMap<String, Vec<u8>>;

impl Dir {
    fn files(self, s: &mut State) -> &mut Files {
        match self {
            Dir::Storage => &mut s.persisted.files,
            Dir::App => &mut s.persisted.app_files,
        }
    }

    pub(crate) fn name_ok(self, b: &[u8]) -> bool {
        match self {
            Dir::Storage => storage_name_ok(b),
            Dir::App => std::str::from_utf8(b).is_ok_and(app_file_name_ok),
        }
    }

    /// The stored spelling of `name`: the card's app folder is case-insensitive, storage names are lower case.
    fn find(self, files: &Files, name: &str) -> Option<String> {
        match self {
            Dir::Storage => files.contains_key(name).then(|| name.to_string()),
            Dir::App => {
                let key = app_file_key(name);
                files.keys().find(|k| app_file_key(k) == key).cloned()
            }
        }
    }
}

/// A `ctx.storage` name: 1-64 bytes of `[a-z0-9._-]`, not starting or ending with `.`.
pub fn storage_name_ok(b: &[u8]) -> bool {
    (1..=64).contains(&b.len())
        && b.iter()
            .all(|c| matches!(c, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
        && b[0] != b'.'
        && b[b.len() - 1] != b'.'
}

/// A `ctx.files` name: 1-128 bytes, no controls or `/ \ : * ? " < > | ~`, no leading `.`, no trailing `.` or
/// space, and not a device name before the first dot.
pub fn app_file_name_ok(name: &str) -> bool {
    const DEVICES: [&str; 4] = ["con", "prn", "aux", "nul"];
    if name.is_empty() || name.len() > 128 || name.starts_with('.') || name.ends_with(['.', ' ']) {
        return false;
    }
    if name
        .chars()
        .any(|c| c.is_control() || "/\\:*?\"<>|~".contains(c))
    {
        return false;
    }
    let stem = name.split('.').next().unwrap_or(name).to_lowercase();
    let numbered = |p: &str| {
        stem.strip_prefix(p)
            .is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
    };
    !(DEVICES.contains(&stem.as_str()) || numbered("com") || numbered("lpt"))
}

/// What two app file names must share to be the same file.
pub fn app_file_key(name: &str) -> String {
    name.to_lowercase()
}

/// `write` or `append` into `dir` with the per-call limit of `ctx.storage:write` and `ctx.files:write`.
pub(crate) fn store(
    s: &mut State,
    dir: Dir,
    name: &[u8],
    data: &[u8],
    mode: Mode,
) -> Result<(), &'static str> {
    store_capped(s, dir, name, data, mode, STORAGE_WRITE_MAX)
}

/// [`store`] with another per-call limit (`net` `save` into the app folder allows more); a storage file never
/// takes more than [`STORAGE_WRITE_MAX`].
pub(crate) fn store_capped(
    s: &mut State,
    dir: Dir,
    name: &[u8],
    data: &[u8],
    mode: Mode,
    max_call: usize,
) -> Result<(), &'static str> {
    if !dir.name_ok(name) {
        return Err("invalid_argument");
    }
    let name = std::str::from_utf8(name).expect("checked by name_ok");
    let max_call = match dir {
        Dir::Storage => max_call.min(STORAGE_WRITE_MAX),
        Dir::App => max_call,
    };
    if data.len() > max_call {
        return Err("limit_exceeded");
    }
    let (max_bytes, max_files) = s.cfg.storage_quota;
    let files = dir.files(s);
    let key = dir.find(files, name);
    let body = match (&key, mode) {
        (Some(k), Mode::Append) => [files[k].as_slice(), data].concat(),
        _ => data.to_vec(),
    };
    if body.len() as u64 > APP_FILE_MAX {
        return Err("limit_exceeded");
    }
    if key.is_none() && files.len() >= max_files {
        return Err("limit_exceeded");
    }
    if dir == Dir::Storage {
        let others: u64 = files
            .iter()
            .filter(|(k, _)| Some(*k) != key.as_ref())
            .map(|(_, v)| v.len() as u64)
            .sum();
        if body.len() as u64 + others > max_bytes {
            return Err("limit_exceeded");
        }
    }
    let key = key.unwrap_or_else(|| name.to_string());
    if files.get(&key) != Some(&body) {
        files.insert(key, body);
        s.persist_changed = true;
    }
    Ok(())
}

/// The file name in a method's first argument, if valid for `dir`.
fn target(dir: Dir, v: &Value) -> Option<String> {
    raw(v)
        .filter(|b| dir.name_ok(b))
        .map(|b| String::from_utf8(b).expect("checked by name_ok"))
}

/// `write` or `append`: the file as given, or the given data added to its end.
fn writer(st: &Shared, dir: Dir, mode: Mode) -> Method {
    let st = st.clone();
    method(move |lua, args| {
        let a = args.into_vec();
        let (Some(name), Some(data)) = (raw(arg(&a, 0)), raw(arg(&a, 1))) else {
            return soft_fail(lua, "invalid_argument");
        };
        match with(&st, |s| store(s, dir, &name, &data, mode)) {
            Ok(()) => true.into_lua_multi(lua),
            Err(code) => soft_fail(lua, code),
        }
    })
}

/// An optional integer argument: nil gives `default`, a float counts if it is whole.
fn opt_int(v: &Value, default: i64) -> Option<i64> {
    match v {
        Value::Nil => Some(default),
        Value::Integer(i) => Some(i64::from(*i)),
        Value::Number(n) if n.fract() == 0.0 && n.abs() <= i32::MAX as f32 => Some(*n as i64),
        _ => None,
    }
}

/// `files:read(name, offset, len)`: at most `len` bytes from `offset`.
fn range_reader(st: &Shared) -> Method {
    let st = st.clone();
    method(move |lua, args| {
        let a = args.into_vec();
        let Some(name) = target(Dir::App, arg(&a, 0)) else {
            return soft_fail(lua, "invalid_argument");
        };
        let (Some(offset), Some(len)) = (opt_int(arg(&a, 1), 0), opt_int(arg(&a, 2), READ_DEFAULT))
        else {
            return soft_fail(lua, "invalid_argument");
        };
        if offset < 0 || len < 1 {
            return soft_fail(lua, "invalid_argument");
        }
        if len > READ_MAX {
            return soft_fail(lua, "limit_exceeded");
        }
        let data = with(&st, |s| {
            let files = Dir::App.files(s);
            let key = Dir::App.find(files, &name)?;
            let d = &files[&key];
            let from = usize::try_from(offset).map_or(d.len(), |o| o.min(d.len()));
            let to = from.saturating_add(len as usize).min(d.len());
            Some(d[from..to].to_vec())
        });
        match data {
            Some(d) => lua.create_string(d)?.into_lua_multi(lua),
            None => soft_fail(lua, "not_found"),
        }
    })
}

/// The methods both directories have.
fn methods(st: &Shared, dir: Dir) -> Vec<(&'static str, Method)> {
    let write = writer(st, dir, Mode::Write);
    let read = if dir == Dir::App {
        range_reader(st)
    } else {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            let Some(name) = target(dir, arg(&a, 0)) else {
                return soft_fail(lua, "invalid_argument");
            };
            if a.iter().skip(1).any(|v| !v.is_nil()) {
                with(&st, |s| {
                    s.warn("storage:read: offset/length ignored; the device always returns the whole file")
                });
            }
            match with(&st, |s| {
                dir.find(dir.files(s), &name)
                    .map(|k| dir.files(s)[&k].clone())
            }) {
                Some(data) => lua.create_string(data)?.into_lua_multi(lua),
                None => soft_fail(lua, "not_found"),
            }
        })
    };
    let exists = {
        let st = st.clone();
        method(move |lua, args| {
            let name = target(dir, args.iter().next().unwrap_or(&Value::Nil));
            let found = name.is_some_and(|n| with(&st, |s| dir.find(dir.files(s), &n).is_some()));
            found.into_lua_multi(lua)
        })
    };
    let size = {
        let st = st.clone();
        method(move |lua, args| {
            let Some(name) = target(dir, args.iter().next().unwrap_or(&Value::Nil)) else {
                return soft_fail(lua, "invalid_argument");
            };
            let n = with(&st, |s| {
                let files = dir.files(s);
                dir.find(files, &name).map(|k| files[&k].len())
            });
            match n {
                Some(n) => int_value(n as i64).into_lua_multi(lua),
                None => soft_fail(lua, "not_found"),
            }
        })
    };
    let remove = {
        let st = st.clone();
        method(move |lua, args| {
            let Some(name) = target(dir, args.iter().next().unwrap_or(&Value::Nil)) else {
                return soft_fail(lua, "invalid_argument");
            };
            with(&st, |s| {
                let files = dir.files(s);
                if let Some(key) = dir.find(files, &name) {
                    files.remove(&key);
                    s.persist_changed = true;
                }
            });
            true.into_lua_multi(lua)
        })
    };
    let list = {
        let st = st.clone();
        method(move |lua, _| {
            let items: Vec<(String, usize)> = with(&st, |s| {
                dir.files(s)
                    .iter()
                    .map(|(k, v)| (k.clone(), v.len()))
                    .collect()
            });
            let t = lua.create_table()?;
            for (name, size) in items {
                if dir == Dir::Storage {
                    t.raw_push(name)?;
                } else {
                    let item = lua.create_table()?;
                    item.raw_set("name", name)?;
                    item.raw_set("size", int_value(size as i64))?;
                    t.raw_push(item)?;
                }
            }
            t.into_lua_multi(lua)
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
            let f = Dir::Storage.files(s);
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
    let mut storage_methods = methods(st, Dir::Storage);
    storage_methods.push(("usage", usage(st)));
    ctx.raw_set("storage", new_object(lua, "storage", &storage_methods)?)?;
    let mut file_methods = methods(st, Dir::App);
    file_methods.push(("append", writer(st, Dir::App, Mode::Append)));
    let files = new_object(lua, "files", &file_methods)?;
    let app_id = with(st, |s| s.app_id.clone());
    files.raw_set("path", format!("/AppFiles/{app_id}"))?;
    ctx.raw_set("files", files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_rules_are_separate_per_directory() {
        assert!(storage_name_ok(b"a.b-c_1") && !storage_name_ok(b"A") && !storage_name_ok(b"a b"));
        assert!(!storage_name_ok(&[b'a'; 65]) && storage_name_ok(&[b'a'; 64]));
        assert!(app_file_name_ok("Мой файл.TXT") && app_file_name_ok("a b"));
        assert!(!app_file_name_ok(&"я".repeat(65)) && app_file_name_ok(&"я".repeat(64)));
        for bad in ["COM1.x", "lpt9", "Nul", ".a", "a.", "a ", "a~", "a\u{7f}"] {
            assert!(!app_file_name_ok(bad), "{bad}");
        }
        for ok in ["COM0", "com10", "x.con", "console"] {
            assert!(app_file_name_ok(ok), "{ok}");
        }
    }

    #[test]
    fn storage_never_takes_more_than_its_write_limit() {
        use crate::app::AppFiles;
        use crate::config::{Config, APP_FILE_SAVE_MAX};
        use crate::{Core, Persisted};
        let app = AppFiles(BTreeMap::from([
            (
                "manifest.json".to_string(),
                br#"{"app_id": "t", "entry": "index.lua", "api": "2.2"}"#.to_vec(),
            ),
            ("index.lua".to_string(), b"".to_vec()),
        ]));
        let core = Core::new(app, Config::default(), Persisted::default());
        let big = vec![b'x'; STORAGE_WRITE_MAX + 1];
        with(&core.st, |s| {
            let cap = APP_FILE_SAVE_MAX;
            assert_eq!(
                store_capped(s, Dir::Storage, b"a", &big, Mode::Write, cap),
                Err("limit_exceeded")
            );
            assert_eq!(
                store_capped(s, Dir::App, b"a", &big, Mode::Write, cap),
                Ok(())
            );
        });
    }
}
