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

/// A Lua float as the f64 written with the f32's shortest decimal digits, so `0.1` stays `0.1` (0.1 had doubles).
pub fn widen(x: f32) -> f64 {
    if !x.is_finite() {
        return f64::from(x);
    }
    format!("{x:e}").parse().unwrap_or(f64::from(x))
}

/// Python `repr()` of a float: shortest digits, exponent form below 1e-4 and from 1e16 on.
pub fn py_float(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.into();
    }
    let s = format!("{x:e}");
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let (sign, mant) = mant.strip_prefix('-').map_or(("", mant), |m| ("-", m));
    let digits: String = mant.chars().filter(|&c| c != '.').collect();
    if (-4..16).contains(&exp) {
        let point = exp + 1;
        if point <= 0 {
            format!(
                "{sign}0.{}{digits}",
                "0".repeat(point.unsigned_abs() as usize)
            )
        } else if point as usize >= digits.len() {
            let zeros = "0".repeat(point as usize - digits.len());
            format!("{sign}{digits}{zeros}.0")
        } else {
            let (int, frac) = digits.split_at(point as usize);
            format!("{sign}{int}.{frac}")
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let dot = if rest.is_empty() { "" } else { "." };
        let esign = if exp < 0 { '-' } else { '+' };
        format!("{sign}{first}{dot}{rest}e{esign}{:02}", exp.abs())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_print_as_python_repr() {
        for (x, want) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (123.456, "123.456"),
            (0.0001, "0.0001"),
            (1e-5, "1e-05"),
            (1.5e-7, "1.5e-07"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (2.5e300, "2.5e+300"),
            (f64::NAN, "nan"),
            (f64::NEG_INFINITY, "-inf"),
        ] {
            assert_eq!(py_float(x), want);
        }
        assert_eq!(py_float(widen(0.1)), "0.1");
        assert_eq!(py_float(widen(16_777_217.0)), "16777216.0");
    }
}
