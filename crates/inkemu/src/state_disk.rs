//! The app's data on disk: `state.json`, `lock.json` and `files/`, as 0.1 laid them out.
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use inkcore::log::py_repr_str;
use inkcore::{valid_name, Persisted};

/// `state.json` or `lock.json`; a read error other than a missing file is returned worded as 0.1's OSError.
fn read_opt(p: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(p) {
        Ok(data) => Ok(Some(data)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(py_os_error(&e, p)),
    }
}

/// Python's `str(OSError)`: `[Errno 21] Is a directory: '/path'`.
fn py_os_error(e: &io::Error, p: &Path) -> String {
    let text = e.to_string();
    let Some(code) = e.raw_os_error() else {
        return text;
    };
    let msg = text
        .strip_suffix(&format!(" (os error {code})"))
        .unwrap_or(&text);
    format!(
        "[Errno {code}] {msg}: {}",
        py_repr_str(&p.display().to_string())
    )
}

/// The app data on disk, and why `state.json` could not be read (for the core's "unreadable state" warning).
pub fn load(state_dir: &Path) -> (Persisted, Option<String>) {
    let mut files = BTreeMap::new();
    if let Ok(rd) = fs::read_dir(state_dir.join("files")) {
        for e in rd.flatten() {
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            if e.path().is_file() && valid_name(name.as_bytes()) {
                if let Ok(data) = fs::read(e.path()) {
                    files.insert(name, data);
                }
            }
        }
    }
    let (state_json, state_error) = match read_opt(&state_dir.join("state.json")) {
        Ok(data) => (data, None),
        Err(e) => (None, Some(e)),
    };
    let persisted = Persisted {
        state_json,
        lock_json: read_opt(&state_dir.join("lock.json")).unwrap_or_default(),
        files,
    };
    (persisted, state_error)
}

fn write_atomic(path: &Path, tmp: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(tmp, data)?;
    fs::rename(tmp, path)
}

/// Writes what differs from `old` (the last data known to be on disk).
pub fn save(state_dir: &Path, new: &Persisted, old: &Persisted) -> io::Result<()> {
    for (name, new, old) in [
        ("state.json", &new.state_json, &old.state_json),
        ("lock.json", &new.lock_json, &old.lock_json),
    ] {
        if new == old {
            continue;
        }
        match new {
            Some(data) => write_atomic(
                &state_dir.join(name),
                &state_dir.join(format!("{name}.tmp")),
                data,
            )?,
            None => remove(&state_dir.join(name))?,
        }
    }
    let root = state_dir.join("files");
    for (name, data) in &new.files {
        if old.files.get(name) != Some(data) {
            write_atomic(
                &root.join(name),
                &root.join(format!("{name}.inkemu-tmp")),
                data,
            )?;
        }
    }
    for name in old.files.keys().filter(|n| !new.files.contains_key(*n)) {
        remove(&root.join(name))?;
    }
    Ok(())
}

fn remove(p: &Path) -> io::Result<()> {
    match fs::remove_file(p) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// `fresh`: forget saved state, lock data, files and screenshots.
pub fn wipe(state_dir: &Path) {
    for name in ["state.json", "lock.json"] {
        let _ = remove(&state_dir.join(name));
    }
    for name in ["files", "shots"] {
        let _ = fs::remove_dir_all(state_dir.join(name));
    }
}

/// Copies `<fixtures>/files/*` into the app's files folder, never overwriting.
pub fn seed_files(fixtures_dir: &Path, state_dir: &Path) {
    let Ok(rd) = fs::read_dir(fixtures_dir.join("files")) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    let dst = state_dir.join("files");
    for e in entries {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        let is_file = e.file_type().is_ok_and(|t| t.is_file());
        if is_file && valid_name(name.as_bytes()) && !dst.join(&name).exists() {
            let _ = fs::create_dir_all(&dst);
            let _ = fs::copy(e.path(), dst.join(&name));
        }
    }
}
