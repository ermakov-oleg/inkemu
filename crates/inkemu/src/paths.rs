//! Where state lives, as 0.1 `paths.py`.
use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

/// `.emu` in the current directory, or `$INKEMU_STATE_ROOT`.
pub fn state_root() -> PathBuf {
    let root = std::env::var_os("INKEMU_STATE_ROOT").unwrap_or_else(|| ".emu".into());
    absolute(Path::new(&root))
}

/// The manifest's app_id if it is Latin letters only, else the letters of the directory name.
/// `manifest` None reads `manifest.json` from `app_dir`.
pub fn app_id_of(app_dir: &Path, manifest: Option<&Map<String, Value>>) -> String {
    // app_id comes from an untrusted manifest and names directories we may wipe
    let read;
    let manifest = match manifest {
        Some(m) => Some(m),
        None => {
            read = std::fs::read(app_dir.join("manifest.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
            read.as_ref().and_then(Value::as_object)
        }
    };
    match manifest.and_then(|m| m.get("app_id")) {
        Some(Value::String(id))
            if !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphabetic()) =>
        {
            id.clone()
        }
        _ => fallback_app_id(app_dir),
    }
}

/// The letters of the directory's name, or "app".
pub fn fallback_app_id(app_dir: &Path) -> String {
    let name = resolve(app_dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let letters: String = name.chars().filter(char::is_ascii_alphabetic).collect();
    if letters.is_empty() {
        "app".into()
    } else {
        letters
    }
}

/// The 0.1 refusal of a derived state dir that escapes the state root.
pub fn check_inside(state_dir: &Path, root: &Path) -> Result<(), String> {
    if resolve(state_dir).starts_with(resolve(root)) {
        Ok(())
    } else {
        Err(format!(
            "state dir {} is outside the state root {}",
            state_dir.display(),
            root.display()
        ))
    }
}

pub fn absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

/// Python's non-strict `Path.resolve()`: `..` and symlinks resolved as far as the path exists.
pub fn resolve(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in absolute(p).components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    let mut tail = vec![];
    let mut head = out.as_path();
    loop {
        if let Ok(real) = head.canonicalize() {
            return tail.iter().rev().fold(real, |acc, t| acc.join(t));
        }
        match (head.parent(), head.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_owned());
                head = parent;
            }
            _ => return out,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_id_comes_from_the_manifest_or_the_directory_letters() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("my-app_2");
        std::fs::create_dir(&app).unwrap();
        assert_eq!(app_id_of(&app, None), "myapp");
        std::fs::write(app.join("manifest.json"), r#"{"app_id": "clock"}"#).unwrap();
        assert_eq!(app_id_of(&app, None), "clock");
        std::fs::write(app.join("manifest.json"), r#"{"app_id": "../victim"}"#).unwrap();
        assert_eq!(app_id_of(&app, None), "myapp");
        assert_eq!(app_id_of(&dir.path().join("123"), None), "app");
    }

    #[test]
    fn a_state_dir_outside_the_root_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        assert!(check_inside(&root.join("clock"), &root).is_ok());
        let err = check_inside(&root.join("..").join("victim"), &root).unwrap_err();
        assert!(
            err.starts_with("state dir ") && err.contains(" is outside the state root "),
            "{err}"
        );
    }
}
