use std::fs;
use std::path::{Path, PathBuf};

use inkemu::{Session, SessionOptions};
use serde_json::json;

const COUNTER: &str = "ctx.state.n = (ctx.state.n or 0) + 1\n";

fn write_app(dir: &Path, index_lua: &str) -> PathBuf {
    let app = dir.join("my-app");
    fs::create_dir_all(&app).unwrap();
    fs::write(
        app.join("manifest.json"),
        r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.2", "permissions": []}"#,
    )
    .unwrap();
    fs::write(app.join("index.lua"), index_lua).unwrap();
    app
}

fn opts(dir: &Path) -> SessionOptions {
    SessionOptions {
        state_dir: Some(dir.join("state")),
        fixtures_dir: Some(dir.join("fixtures")),
        start_time: Some("2026-11-20T09:00:00".into()),
        ..SessionOptions::default()
    }
}

#[test]
fn state_survives_drop_and_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(tmp.path(), &format!("function on_load(c) {COUNTER} end\n"));
    let mut s = Session::open(&app, opts(tmp.path())).unwrap();
    assert!(s.core().errors().is_empty());
    s.leave();
    drop(s);
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(tmp.path().join("state/state.json")).unwrap()).unwrap();
    assert_eq!(saved, json!({"n": 1}));
    let mut s = Session::open(&app, opts(tmp.path())).unwrap();
    assert_eq!(s.lua_eval("return ctx.state.n").unwrap(), json!(2));
}

#[test]
fn state_is_not_written_before_leave_or_reload() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(tmp.path(), &format!("function on_load(c) {COUNTER} end\n"));
    let s = Session::open(&app, opts(tmp.path())).unwrap();
    assert!(!tmp.path().join("state/state.json").exists());
    drop(s);
}

#[test]
fn reload_saves_state_and_picks_up_edited_code() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(
        tmp.path(),
        "function on_load(c) ctx.state.v = 1 end error('broken')\n",
    );
    let mut s = Session::open(&app, opts(tmp.path())).unwrap();
    assert!(!s.status().running);
    assert!(s.core().errors()[0].contains("broken"));
    fs::write(
        app.join("index.lua"),
        "function on_load(c) ctx.state.v = 7 end\n",
    )
    .unwrap();
    s.reload();
    assert!(s.status().running, "{:?}", &*s.core().errors());
    assert_eq!(s.lua_eval("return ctx.state.v").unwrap(), json!(7));
    s.reload();
    let saved = fs::read(tmp.path().join("state/state.json")).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&saved).unwrap(),
        json!({"v": 7})
    );
}

#[test]
fn fresh_wipes_state_lock_files_and_shots() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(tmp.path(), &format!("function on_load(c) {COUNTER} end\n"));
    let mut s = Session::open(&app, opts(tmp.path())).unwrap();
    s.leave();
    let state = tmp.path().join("state");
    for sub in ["files", "shots"] {
        fs::create_dir_all(state.join(sub)).unwrap();
        fs::write(state.join(sub).join("keep.txt"), "x").unwrap();
    }
    fs::write(state.join("lock.json"), "{}").unwrap();
    drop(s);
    let mut s = Session::open(
        &app,
        SessionOptions {
            fresh: true,
            ..opts(tmp.path())
        },
    )
    .unwrap();
    assert_eq!(s.lua_eval("return ctx.state.n").unwrap(), json!(1));
    for gone in ["state.json", "lock.json", "files", "shots"] {
        assert!(!state.join(gone).exists(), "{gone}");
    }
}

#[test]
fn seeding_copies_fixture_files_without_overwriting() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(tmp.path(), "");
    let files = tmp.path().join("fixtures/files");
    fs::create_dir_all(&files).unwrap();
    fs::write(files.join("{{cal}}.ics"), "seed").unwrap();
    fs::write(files.join("has space"), "bad name").unwrap();
    let fresh = |seed| SessionOptions {
        fresh: true,
        seed_files: seed,
        ..opts(tmp.path())
    };
    drop(Session::open(&app, fresh(None)).unwrap());
    let seeded = tmp.path().join("state/files/{{cal}}.ics");
    assert_eq!(fs::read(&seeded).unwrap(), b"seed");
    assert!(!tmp.path().join("state/files/has space").exists());
    // not fresh and not asked to seed: nothing is copied over what the app has
    fs::write(&seeded, "app wrote this").unwrap();
    drop(
        Session::open(
            &app,
            SessionOptions {
                fresh: false,
                ..fresh(None)
            },
        )
        .unwrap(),
    );
    assert_eq!(fs::read(&seeded).unwrap(), b"app wrote this");
    drop(
        Session::open(
            &app,
            SessionOptions {
                fresh: false,
                ..fresh(Some(true))
            },
        )
        .unwrap(),
    );
    assert_eq!(fs::read(&seeded).unwrap(), b"app wrote this");
    // fresh without seeding leaves the folder empty
    drop(Session::open(&app, fresh(Some(false))).unwrap());
    assert!(!seeded.exists());
}

#[test]
fn derived_state_dir_stays_under_the_root_whatever_the_app_id() {
    let tmp = tempfile::tempdir().unwrap();
    let victim = tmp.path().join("victim");
    fs::create_dir_all(victim.join("files")).unwrap();
    fs::write(victim.join("files/keep.txt"), "x").unwrap();
    fs::write(victim.join("state.json"), "{}").unwrap();
    let root = tmp.path().join("root");
    // this is the only test that sets the variable; the others pass their dirs explicitly
    std::env::set_var("INKEMU_STATE_ROOT", &root);
    let app = tmp.path().join("my-app");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("manifest.json"), r#"{"app_id": "../victim"}"#).unwrap();
    let s = Session::open(
        &app,
        SessionOptions {
            fresh: true,
            ..SessionOptions::default()
        },
    )
    .unwrap();
    assert!(s.core().errors()[0].contains("app_id"));
    assert!(!s.status().running);
    assert!(victim.join("files/keep.txt").exists() && victim.join("state.json").exists());
    assert_eq!(s.state_dir(), root.join("myapp"));
    assert_eq!(s.secrets_file(), root.join("secrets.json"));
    assert!(s.fixtures_dir().ends_with("tests/fixtures/myapp"));
}

#[test]
fn missing_manifest_names_the_folder_and_app_id_uses_its_letters() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("no-manifest_1");
    fs::create_dir_all(&app).unwrap();
    let s = Session::open(&app, opts(tmp.path())).unwrap();
    let dir = fs::canonicalize(&app).unwrap();
    assert_eq!(
        s.core().errors()[0],
        format!("manifest.json not found in {}", dir.display())
    );
    assert_eq!(s.status().app_id, "nomanifest");
}

#[test]
fn bad_net_mode_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(tmp.path(), "");
    let err = Session::open(
        &app,
        SessionOptions {
            net: "x".into(),
            ..opts(tmp.path())
        },
    )
    .err()
    .unwrap();
    assert_eq!(err, "net must be one of replay, record, live, not 'x'");
}

#[test]
fn start_time_sets_the_virtual_clock() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(tmp.path(), "");
    let mut s = Session::open(&app, opts(tmp.path())).unwrap();
    assert_eq!(s.status().time, "2026-11-20T09:00:00.000");
    s.wait(1500).unwrap();
    assert_eq!(s.status().time, "2026-11-20T09:00:01.500");
    let bad = Session::open(
        &app,
        SessionOptions {
            start_time: Some("nope".into()),
            ..opts(tmp.path())
        },
    );
    assert_eq!(bad.err().unwrap(), "Invalid isoformat string: 'nope'");
}
