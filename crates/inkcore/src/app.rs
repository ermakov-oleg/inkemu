//! The app package: its files and its manifest.
use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::log::{py_repr, py_str};

pub const PERMISSIONS: [&str; 3] = ["net", "reader", "ble"];

/// The app folder: POSIX paths relative to the folder → file contents.
#[derive(Clone, Debug, Default)]
pub struct AppFiles(pub BTreeMap<String, Vec<u8>>);

impl AppFiles {
    pub fn get(&self, rel: &str) -> Option<&[u8]> {
        self.0.get(rel).map(Vec::as_slice)
    }

    /// A file by a path as an app writes it (`./a/../b.lua`): its normalised path and contents,
    /// or None when it does not exist or points outside the folder.
    pub fn find(&self, rel: &str) -> Option<(String, &[u8])> {
        if rel.starts_with('/') {
            return None;
        }
        let mut parts: Vec<&str> = vec![];
        for p in rel.split('/') {
            match p {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                p => parts.push(p),
            }
        }
        let path = parts.join("/");
        let data = self.get(&path)?;
        Some((path, data))
    }
}

/// `require` lookup as 0.1 `_find_module`: (path, source) or the searcher's "no file" message.
pub fn find_module<'a>(files: &'a AppFiles, name: &str) -> Result<(String, &'a [u8]), String> {
    let rel = name.replace('.', "/") + ".lua";
    let valid = name.split('.').all(|seg| {
        !seg.is_empty()
            && seg
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    });
    if valid {
        if let Some(found) = files.find(&rel) {
            return Ok(found);
        }
    }
    Err(format!("\n\tno file '{rel}' in the app folder"))
}

fn latin_letters(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphabetic())
}

/// The manifest's app_id if it is Latin letters only, else `fallback` (the letters of the app folder's name).
pub fn app_id_of(manifest: &Map<String, Value>, fallback: &str) -> String {
    match manifest.get("app_id") {
        Some(Value::String(id)) if latin_letters(id) => id.clone(),
        _ => fallback.to_string(),
    }
}

/// `major.minor` with optional surrounding whitespace, digits kept as written.
fn parse_api(s: &str) -> Option<(&str, &str)> {
    let (a, b) = s.trim().split_once('.')?;
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit());
    (digits(a) && digits(b)).then_some((a, b))
}

/// The API a manifest declares, as numbers (`0.8` when it says none; a malformed one fails the install).
pub(crate) fn api_of(m: &Map<String, Value>) -> (u32, u32) {
    let text = m.get("api").map_or_else(|| "0.8".into(), py_str);
    parse_api(&text).map_or((0, 8), |(a, b)| {
        let n = |p: &str| p.parse().unwrap_or(u32::MAX);
        (n(a), n(b))
    })
}

/// Compares decimal digit strings of any length by value.
fn cmp_digits(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// The manifest and the install problem, as 0.1 `Emu._read_manifest` (messages verbatim).
pub fn read_manifest(
    files: &AppFiles,
    firmware_api: (u8, u8),
    dir_label: &str,
) -> (Map<String, Value>, Option<String>) {
    let Some(raw) = files.get("manifest.json") else {
        return (
            Map::new(),
            Some(format!("manifest.json not found in {dir_label}")),
        );
    };
    let m = match serde_json::from_slice::<Value>(raw) {
        Ok(Value::Object(m)) => m,
        Ok(_) => {
            return (
                Map::new(),
                Some("manifest.json must be a JSON object".into()),
            )
        }
        Err(e) => {
            return (
                Map::new(),
                Some(format!("manifest.json is not valid JSON: {e}")),
            )
        }
    };
    let mut problems = vec![];
    let app_id = m.get("app_id").unwrap_or(&Value::Null);
    if !matches!(app_id, Value::String(id) if latin_letters(id)) {
        problems.push(format!(
            "app_id {} must be Latin letters only",
            py_repr(app_id)
        ));
    }
    let bad: Vec<&Value> = match m.get("permissions") {
        None => vec![],
        Some(Value::Array(perms)) => perms
            .iter()
            .filter(|p| !matches!(p, Value::String(s) if PERMISSIONS.contains(&s.as_str())))
            .collect(),
        Some(other) => vec![other],
    };
    if !bad.is_empty() {
        let bad: Vec<String> = bad.into_iter().map(py_str).collect();
        problems.push(format!(
            "unknown permission(s) {}; allowed: {}",
            bad.join(", "),
            PERMISSIONS.join(", ")
        ));
    }
    let default_api = Value::String("0.8".into());
    let api = m.get("api").unwrap_or(&default_api);
    let api_text = py_str(api);
    let (fw_major, fw_minor) = firmware_api;
    match parse_api(&api_text) {
        None => problems.push(format!("api {} is not 'major.minor'", py_repr(api))),
        Some((major, minor)) => {
            let newer = cmp_digits(major, &fw_major.to_string())
                .then_with(|| cmp_digits(minor, &fw_minor.to_string()))
                .is_gt();
            if newer {
                problems.push(format!(
                    "app needs firmware with API {major}.{minor}; the emulated firmware has {fw_major}.{fw_minor}"
                ));
            }
        }
    }
    let problem = (!problems.is_empty()).then(|| format!("manifest.json: {}", problems.join("; ")));
    (m, problem)
}
