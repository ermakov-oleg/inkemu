//! The emulator log and the Python-style value formatting its 0.1 messages use.
use serde_json::Value;

use crate::clock::DateTime;

#[derive(Default)]
pub struct Log {
    pub log: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub toasts: Vec<String>,
}

impl Log {
    pub fn line(&mut self, now: DateTime, msg: &str) {
        let (_, _, _, h, mi, s, ms) = now;
        self.log
            .push(format!("{h:02}:{mi:02}:{s:02}.{ms:03} {msg}"));
    }
}

/// Python `repr()` of a string.
pub fn py_repr_str(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c == '\x7f' => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

/// Python `repr()` of a value parsed from JSON.
pub fn py_repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => py_repr_str(s),
        Value::Array(a) => format!("[{}]", a.iter().map(py_repr).collect::<Vec<_>>().join(", ")),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("{}: {}", py_repr_str(k), py_repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Python `str()` of a value parsed from JSON.
pub fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        v => py_repr(v),
    }
}
