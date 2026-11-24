//! Lua values <-> JSON with 0.1's rules (`lua_to_py`, `py_to_lua`, `json.dumps`), for `ctx.json`, `ctx.state` and
//! the `lua_eval` test helper.
use std::ffi::c_void;

use mlua::{Lua, Table, Value};
use serde_json::{Map, Number, Value as J};

use crate::log::{py_float, widen};

const MAX_DEPTH: usize = 64;

/// 0.1 `lua_to_py`: 1..n tables become arrays, other tables objects with string keys. Functions, threads,
/// userdata, NaN/inf, strings that are not UTF-8, cycles and too-deep nesting are skipped with a warning naming the
/// key path. `null` is the `ctx.json.null` sentinel. None when `v` itself is skipped.
pub(crate) fn lua_to_json(
    v: &Value,
    path: &str,
    null: Option<&Table>,
    warn: &mut dyn FnMut(String),
) -> Option<J> {
    let mut conv = ToJson {
        null: null.map(Table::to_pointer),
        warn,
        seen: Vec::new(),
    };
    conv.value(v, path)
}

struct ToJson<'a> {
    null: Option<*const c_void>,
    warn: &'a mut dyn FnMut(String),
    /// The tables being converted, outermost first.
    seen: Vec<*const c_void>,
}

enum Key {
    Int(i32),
    Float(f32),
    Bool(bool),
    Str(String),
    Other(&'static str),
}

impl Key {
    /// The JSON object key (0.1: `str(k)`, booleans as JSON).
    fn json(&self) -> String {
        match self {
            Key::Bool(b) => b.to_string(),
            k => k.path(),
        }
    }

    /// As 0.1's f-string put it in a warning path.
    fn path(&self) -> String {
        match self {
            Key::Int(i) => i.to_string(),
            Key::Float(f) => py_float(widen(*f)),
            Key::Bool(b) => if *b { "True" } else { "False" }.into(),
            Key::Str(s) => s.clone(),
            Key::Other(t) => format!("<Lua {t}>"),
        }
    }
}

impl ToJson<'_> {
    fn value(&mut self, v: &Value, path: &str) -> Option<J> {
        match v {
            Value::Nil => Some(J::Null),
            Value::Boolean(b) => Some(J::Bool(*b)),
            Value::Integer(i) => Some(J::from(*i)),
            Value::Number(n) => match Number::from_f64(widen(*n)) {
                Some(n) => Some(J::Number(n)),
                None => {
                    (self.warn)(format!(
                        "{path}: {} cannot be saved, skipped",
                        py_float(f64::from(*n))
                    ));
                    None
                }
            },
            Value::String(s) => match s.to_str() {
                Ok(s) => Some(J::String(s.to_string())),
                Err(_) => {
                    (self.warn)(format!(
                        "{path}: a string that is not valid UTF-8 cannot be saved, skipped"
                    ));
                    None
                }
            },
            Value::Table(t) => self.table(t, path),
            other => {
                (self.warn)(format!(
                    "{path}: a {} cannot be saved, skipped",
                    type_of(other)
                ));
                None
            }
        }
    }

    fn table(&mut self, t: &Table, path: &str) -> Option<J> {
        let ptr = t.to_pointer();
        if self.null == Some(ptr) {
            return Some(J::Null);
        }
        if self.seen.contains(&ptr) {
            (self.warn)(format!("{path}: a cycle, skipped"));
            return None;
        }
        if self.seen.len() >= MAX_DEPTH {
            (self.warn)(format!(
                "{path}: nested deeper than {MAX_DEPTH} levels, skipped"
            ));
            return None;
        }
        let entries: Vec<(Value, Value)> = t.pairs::<Value, Value>().flatten().collect();
        self.seen.push(ptr);
        let mut out: Vec<(Key, J)> = Vec::with_capacity(entries.len());
        for (k, x) in entries {
            let key = match k {
                Value::Integer(i) => Key::Int(i),
                Value::Number(f) => Key::Float(f),
                Value::Boolean(b) => Key::Bool(b),
                Value::String(s) => match s.to_str() {
                    Ok(s) => Key::Str(s.to_string()),
                    Err(_) => {
                        let k = escape(&s.as_bytes());
                        (self.warn)(format!(
                            "{path}.{k}: a string that is not valid UTF-8 cannot be saved, skipped"
                        ));
                        continue;
                    }
                },
                other => Key::Other(type_of(&other)),
            };
            let child = format!("{path}.{}", key.path());
            if let Some(val) = self.value(&x, &child) {
                out.push((key, val));
            }
        }
        self.seen.pop();
        let n = out.len();
        let is_list = n > 0
            && out
                .iter()
                .all(|(k, _)| matches!(k, Key::Int(i) if (1..=n as i64).contains(&i64::from(*i))));
        if is_list {
            out.sort_by_key(|(k, _)| match k {
                Key::Int(i) => *i,
                _ => 0,
            });
            return Some(J::Array(out.into_iter().map(|(_, v)| v).collect()));
        }
        let mut map = Map::new();
        for (k, v) in out {
            map.insert(k.json(), v);
        }
        Some(J::Object(map))
    }
}

/// 0.1 `lua_typename` for the values a table entry may hold.
fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Function(_) => "function",
        Value::Thread(_) => "thread",
        Value::Table(_) => "table",
        _ => "userdata",
    }
}

/// 0.1's Lua shim: backslashes and bytes >= 0x80 as `\xhh`.
fn escape(b: &[u8]) -> String {
    let mut out = String::new();
    for &c in b {
        if c == b'\\' || c >= 0x80 {
            out.push_str(&format!("\\x{c:02x}"));
        } else {
            out.push(char::from(c));
        }
    }
    out
}

/// 0.1 `py_to_lua`: JSON null becomes `null`, or nothing (the key vanishes) without a sentinel. `big` is told about
/// an integer that does not fit a Lua integer; it becomes a float.
pub(crate) fn json_to_lua(
    lua: &Lua,
    v: &J,
    null: Option<&Table>,
    big: &mut dyn FnMut(&Number),
) -> mlua::Result<Value> {
    Ok(match v {
        J::Null => null.map_or(Value::Nil, |t| Value::Table(t.clone())),
        J::Bool(b) => Value::Boolean(*b),
        J::Number(n) => {
            if let Some(i) = n.as_i64().and_then(|i| i32::try_from(i).ok()) {
                Value::Integer(i)
            } else {
                if n.is_i64() || n.is_u64() {
                    big(n);
                }
                Value::Number(n.as_f64().unwrap_or(f64::NAN) as f32)
            }
        }
        J::String(s) => Value::String(lua.create_string(s)?),
        J::Array(a) => {
            let t = lua.create_table()?;
            for (i, x) in a.iter().enumerate() {
                if !x.is_null() || null.is_some() {
                    t.raw_set(i + 1, json_to_lua(lua, x, null, big)?)?;
                }
            }
            Value::Table(t)
        }
        J::Object(o) => {
            let t = lua.create_table()?;
            for (k, x) in o {
                if !x.is_null() || null.is_some() {
                    t.raw_set(k.as_str(), json_to_lua(lua, x, null, big)?)?;
                }
            }
            Value::Table(t)
        }
    })
}

/// The warning for a JSON integer that became a float.
pub(crate) fn big_int_warning(n: &Number) -> String {
    format!("JSON number {n} does not fit a 32-bit integer; on the device it loses precision")
}

/// Python `json.dumps(v, ensure_ascii=False, sort_keys=True)`; `compact` uses `separators=(",", ":")`.
pub(crate) fn dump(v: &J, compact: bool) -> String {
    let seps = if compact { (",", ":") } else { (", ", ": ") };
    let mut out = String::new();
    write(&mut out, v, seps);
    out
}

fn write(out: &mut String, v: &J, seps: (&str, &str)) {
    match v {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => out.push_str(&i.to_string()),
            (_, Some(u), _) => out.push_str(&u.to_string()),
            (_, _, f) => out.push_str(&py_float(f.unwrap_or(f64::NAN))),
        },
        J::String(s) => write_str(out, s),
        J::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(seps.0);
                }
                write(out, x, seps);
            }
            out.push(']');
        }
        J::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(seps.0);
                }
                write_str(out, k);
                out.push_str(seps.1);
                write(out, &o[k], seps);
            }
            out.push('}');
        }
    }
}

fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
