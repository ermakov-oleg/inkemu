//! The secrets file and `{{name}}` placeholders (0.1 `net._load_secrets`, `Net.expand`,
//! `Net._warn_unknown_secrets`). Redaction itself lives in `inkcore::fixtures`, which `replay` needs too.
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use inkcore::log::{py_float, py_repr_str};
use serde_json::Value as J;

#[derive(Debug, Default, PartialEq)]
pub struct Secrets {
    /// (name, non-empty value).
    pub values: Vec<(String, String)>,
    /// Why entries cannot be used; never contains a secret value.
    pub problems: Vec<String>,
}

fn valid_name(k: &str) -> bool {
    !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

/// The Python exception class 0.1 named for an unreadable file.
fn os_error_name(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::IsADirectory => "IsADirectoryError",
        ErrorKind::NotADirectory => "NotADirectoryError",
        ErrorKind::PermissionDenied => "PermissionError",
        _ => "OSError",
    }
}

/// 0.1 `_load_secrets`: a missing file is no secrets and no problem.
pub fn load(path: &Path) -> Secrets {
    let f = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let unreadable = |why: &str| Secrets {
        values: Vec::new(),
        problems: vec![format!(
            "{f} cannot be read ({why}); no secrets are applied"
        )],
    };
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) if e.kind() == ErrorKind::NotFound => return Secrets::default(),
        Err(e) => return unreadable(os_error_name(e.kind())),
    };
    let Ok(text) = String::from_utf8(data) else {
        return unreadable("UnicodeDecodeError");
    };
    let Ok(doc) = serde_json::from_str::<J>(&text) else {
        return unreadable("JSONDecodeError");
    };
    let J::Object(map) = doc else {
        return Secrets {
            values: Vec::new(),
            problems: vec![format!("{f} must be a JSON object; no secrets are applied")],
        };
    };
    let mut out = Secrets::default();
    for (k, v) in map {
        let v = match v {
            J::Number(n) => Some(match (n.as_i64(), n.as_u64()) {
                (Some(i), _) => i.to_string(),
                (_, Some(u)) => u.to_string(),
                _ => py_float(n.as_f64().unwrap_or(f64::NAN)),
            }),
            J::String(s) => Some(s),
            _ => None,
        };
        let key = py_repr_str(&k);
        if !valid_name(&k) {
            out.problems.push(format!(
                "{f}: key {key} is not a valid name (letters, digits, _ . -); its value is not redacted"
            ));
        } else if let Some(v) = v.filter(|v| !v.is_empty()) {
            out.values.push((k, v));
        } else {
            out.problems.push(format!(
                "{f}: value of {key} must be a non-empty string or a number; it is not redacted"
            ));
        }
    }
    out
}

/// 0.1 `Net.expand`: `{{name}}` placeholders become the secret values; unknown names stay as they are.
pub fn expand(data: &[u8], secrets: &[(String, String)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i..].starts_with(b"{{") {
            let start = i + 2;
            let mut j = start;
            while j < data.len() && (data[j].is_ascii_alphanumeric() || b"_.-".contains(&data[j])) {
                j += 1;
            }
            if j > start && data[j..].starts_with(b"}}") {
                let name = &data[start..j];
                match secrets.iter().find(|(k, _)| k.as_bytes() == name) {
                    Some((_, v)) => out.extend_from_slice(v.as_bytes()),
                    None => out.extend_from_slice(&data[i..j + 2]),
                }
                i = j + 2;
                continue;
            }
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

pub fn expand_str(text: &str, secrets: &[(String, String)]) -> String {
    String::from_utf8_lossy(&expand(text.as_bytes(), secrets)).into_owned()
}

/// Python `urllib.parse.urlsplit`: (scheme, netloc, path, query).
pub fn urlsplit(url: &str) -> (String, &str, &str, &str) {
    let mut rest = url;
    let mut scheme = String::new();
    if let Some(i) = url.find(':') {
        let s = &url[..i];
        if s.starts_with(|c: char| c.is_ascii_alphabetic())
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
        {
            scheme = s.to_ascii_lowercase();
            rest = &url[i + 1..];
        }
    }
    let mut netloc = "";
    if let Some(r) = rest.strip_prefix("//") {
        let end = r.find(['/', '?', '#']).unwrap_or(r.len());
        netloc = &r[..end];
        rest = &r[end..];
    }
    let rest = rest.split('#').next().unwrap_or_default();
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    (scheme, netloc, path, query)
}

/// 0.1 `Net._warn_unknown_secrets`: path segments and query values of the redacted URL that look like secrets.
pub fn unknown_secret_warnings(shown: &str) -> Vec<String> {
    let (scheme, netloc, path, query) = urlsplit(shown);
    let looks_secret = |s: &str| {
        s.len() >= 20
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    };
    let candidates = path.split('/').map(|s| ("path segment", s)).chain(
        query
            .split('&')
            .map(|v| ("query value", v.split_once('=').map_or("", |p| p.1))),
    );
    candidates
        .filter(|(_, text)| looks_secret(text))
        .map(|(what, _)| {
            format!(
                "net: a {what} of {scheme}://{netloc} looks like a secret but is not in secrets.json; \
                 it will be saved in the fixture as is"
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_expand_and_unknown_ones_stay() {
        let s = vec![("cal".to_string(), "abc".to_string())];
        assert_eq!(expand(b"/{{cal}}/{{x}}/{{{cal}}}", &s), b"/abc/{{x}}/{abc}");
    }

    #[test]
    fn urlsplit_as_python() {
        assert_eq!(
            urlsplit("HTTP://u:p@h:1/a/b?x=1&y=2#f"),
            ("http".to_string(), "u:p@h:1", "/a/b", "x=1&y=2")
        );
        assert_eq!(urlsplit("x"), (String::new(), "", "x", ""));
    }
}
