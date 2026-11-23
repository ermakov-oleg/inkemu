//! The app's data on disk: `state.json`, `lock.json`, `files/` (as 0.1 laid them out) and `appfiles/`.
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use inkcore::log::py_repr_str;
use inkcore::{app_file_key, app_file_name_ok, storage_name_ok, Persisted};

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

/// `ctx.storage` lives in `files/` (as 0.1), `ctx.files` in `appfiles/`: two directories on the device too.
const STORAGE_DIR: &str = "files";
const APP_FILES_DIR: &str = "appfiles";

/// The files of one directory that an app can name. Of names that differ only in case (the device's card has one
/// file for them) the first in byte order is kept.
fn read_dir(
    dir: &Path,
    name_ok: fn(&str) -> bool,
    case_insensitive: bool,
) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    if let Ok(rd) = fs::read_dir(dir) {
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            if !e.path().is_file() || !name_ok(&name) {
                continue;
            }
            if case_insensitive && !seen.insert(app_file_key(&name)) {
                continue;
            }
            if let Ok(data) = fs::read(e.path()) {
                files.insert(name, data);
            }
        }
    }
    files
}

fn storage_name(name: &str) -> bool {
    storage_name_ok(name.as_bytes())
}

/// The app data on disk, and why `state.json` could not be read (for the core's "unreadable state" warning).
pub fn load(state_dir: &Path) -> (Persisted, Option<String>) {
    let (state_json, state_error) = match read_opt(&state_dir.join("state.json")) {
        Ok(data) => (data, None),
        Err(e) => (None, Some(e)),
    };
    let persisted = Persisted {
        state_json,
        lock_json: read_opt(&state_dir.join("lock.json")).unwrap_or_default(),
        files: read_dir(&state_dir.join(STORAGE_DIR), storage_name, false),
        app_files: read_dir(&state_dir.join(APP_FILES_DIR), app_file_name_ok, true),
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
    save_dir(&state_dir.join(STORAGE_DIR), &new.files, &old.files)?;
    save_dir(
        &state_dir.join(APP_FILES_DIR),
        &new.app_files,
        &old.app_files,
    )
}

fn save_dir(
    root: &Path,
    new: &BTreeMap<String, Vec<u8>>,
    old: &BTreeMap<String, Vec<u8>>,
) -> io::Result<()> {
    // Removals first: on a case-insensitive disk "A" removed and "a" created would otherwise delete the new file.
    for name in old.keys().filter(|n| !new.contains_key(*n)) {
        remove(&root.join(name))?;
    }
    for (name, data) in new {
        if old.get(name) != Some(data) {
            write_atomic(
                &root.join(name),
                &root.join(format!("{name}.inkemu-tmp")),
                data,
            )?;
        }
    }
    Ok(())
}

fn remove(p: &Path) -> io::Result<()> {
    match fs::remove_file(p) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// `fresh`: forget saved state, lock data, both file directories and screenshots.
pub fn wipe(state_dir: &Path) {
    for name in ["state.json", "lock.json"] {
        let _ = remove(&state_dir.join(name));
    }
    for name in [STORAGE_DIR, APP_FILES_DIR, "shots"] {
        let _ = fs::remove_dir_all(state_dir.join(name));
    }
}

/// Copies `<fixtures>/files/*` into the app's `ctx.storage` folder, never overwriting.
pub fn seed_files(fixtures_dir: &Path, state_dir: &Path) {
    let Ok(rd) = fs::read_dir(fixtures_dir.join("files")) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    let dst = state_dir.join(STORAGE_DIR);
    for e in entries {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        let is_file = e.file_type().is_ok_and(|t| t.is_file());
        if is_file && storage_name(&name) && !dst.join(&name).exists() {
            let _ = fs::create_dir_all(&dst);
            let _ = fs::copy(e.path(), dst.join(&name));
        }
    }
}
