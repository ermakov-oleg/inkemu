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
    fs::write(files.join("cal.ics"), "seed").unwrap();
    // API 2.33.1 §9 (09/storage_name_invalid): storage names are lower case
    fs::write(files.join("Has.Upper"), "bad name").unwrap();
    let fresh = |seed| SessionOptions {
        fresh: true,
        seed_files: seed,
        ..opts(tmp.path())
    };
    drop(Session::open(&app, fresh(None)).unwrap());
    let seeded = tmp.path().join("state/files/cal.ics");
    assert_eq!(fs::read(&seeded).unwrap(), b"seed");
    assert!(!tmp.path().join("state/files/Has.Upper").exists());
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

    #[cfg(unix)]
    {
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("linked")).unwrap();
        let linked = tmp.path().join("linked");
        fs::create_dir_all(&linked).unwrap();
        fs::write(linked.join("manifest.json"), r#"{"app_id": "linked"}"#).unwrap();
        let err = Session::open(&linked, SessionOptions::default())
            .err()
            .unwrap();
        assert_eq!(
            err,
            format!(
                "state dir {} is outside the state root {}",
                root.join("linked").display(),
                root.display()
            )
        );
    }
}

#[test]
fn a_bad_start_time_leaves_existing_state_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(tmp.path(), "");
    let state = tmp.path().join("state");
    fs::create_dir_all(state.join("files")).unwrap();
    fs::write(state.join("state.json"), "{}").unwrap();
    fs::write(state.join("files/keep.txt"), "x").unwrap();
    let r = Session::open(
        &app,
        SessionOptions {
            fresh: true,
            start_time: Some("nope".into()),
            ..opts(tmp.path())
        },
    );
    assert!(r.is_err());
    assert!(state.join("state.json").exists() && state.join("files/keep.txt").exists());
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

#[test]
fn seeded_files_are_read_and_written_through_ctx_storage() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(
        tmp.path(),
        "function on_load(c) seen = c.storage:read('cal.txt') c.storage:write('out.bin', '\\0\\255') end\n",
    );
    let files = tmp.path().join("fixtures/files");
    fs::create_dir_all(&files).unwrap();
    fs::write(files.join("cal.txt"), "Событие").unwrap();
    let mut s = Session::open(
        &app,
        SessionOptions {
            seed_files: Some(true),
            ..opts(tmp.path())
        },
    )
    .unwrap();
    assert!(
        s.core().errors().is_empty(),
        "{:?}",
        s.core().errors().to_vec()
    );
    assert_eq!(s.lua_eval("return seen").unwrap(), json!("Событие"));
    assert_eq!(
        fs::read(tmp.path().join("state/files/out.bin")).unwrap(),
        [0, 255]
    );
}

#[test]
fn an_unreadable_state_json_warns_and_starts_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(tmp.path(), "");
    let state = tmp.path().join("state/state.json");
    fs::create_dir_all(&state).unwrap();
    let s = Session::open(&app, opts(tmp.path())).unwrap();
    let warnings = s.core().warnings().to_vec();
    let expected = format!(
        "{}: unreadable state ([Errno 21] Is a directory: '{}'), starting with an empty ctx.state",
        state.display(),
        state.display()
    );
    assert_eq!(warnings, [expected]);
    let log = s.core().log().to_vec();
    let warned = log.iter().position(|l| l.contains("unreadable state"));
    let started = log.iter().position(|l| l.contains("app testapp started"));
    assert!(warned < started, "{log:?}");
}

#[test]
fn ctx_files_live_in_appfiles_apart_from_storage_and_fresh_wipes_them() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(
        tmp.path(),
        "function on_load(c)\n\
           if not c.files:exists('a') then c.files:write('a', '1') c.storage:write('b', '2') end\n\
         end\n",
    );
    let seed = tmp.path().join("fixtures/files");
    fs::create_dir_all(&seed).unwrap();
    fs::write(seed.join("cal.txt"), "seeded").unwrap();
    let seeded = SessionOptions {
        seed_files: Some(true),
        ..opts(tmp.path())
    };
    let s = Session::open(&app, seeded).unwrap();
    assert!(
        s.core().errors().is_empty(),
        "{:?}",
        s.core().errors().to_vec()
    );
    drop(s);
    let state = tmp.path().join("state");
    assert_eq!(fs::read(state.join("appfiles/a")).unwrap(), b"1");
    assert_eq!(fs::read(state.join("files/b")).unwrap(), b"2");
    assert!(!state.join("files/a").exists() && !state.join("appfiles/b").exists());

    let mut s = Session::open(&app, opts(tmp.path())).unwrap();
    assert_eq!(
        s.lua_eval("return {ctx.files:read('a'), ctx.storage:read('cal.txt'), ctx.files:exists('cal.txt')}")
            .unwrap(),
        json!(["1", "seeded", false])
    );
    s.lua_exec("ctx.files:remove('a')").unwrap();
    assert!(!state.join("appfiles/a").exists());
    s.lua_exec("ctx.files:write('z', '9')").unwrap();
    assert!(state.join("appfiles/z").exists());
    drop(s);

    let fresh = SessionOptions {
        fresh: true,
        ..opts(tmp.path())
    };
    let _s = Session::open(&app, fresh).unwrap();
    assert!(!state.join("appfiles/z").exists());
}

#[test]
fn app_files_on_disk_keep_the_creation_spelling_and_one_file_per_name() {
    let tmp = tempfile::tempdir().unwrap();
    let app = write_app(
        tmp.path(),
        "function on_load(c)\n\
           if not c.files:exists('Book.TXT') then c.files:write('Book.TXT', '1') end\n\
         end\n",
    );
    drop(Session::open(&app, opts(tmp.path())).unwrap());
    let dir = tmp.path().join("state/appfiles");
    assert_eq!(fs::read(dir.join("Book.TXT")).unwrap(), b"1");

    let mut s = Session::open(&app, opts(tmp.path())).unwrap();
    s.lua_exec("ctx.files:write('book.txt', '22')").unwrap();
    assert_eq!(
        s.lua_eval("local l = ctx.files:list() return {#l, l[1].name, ctx.files:read('BOOK.txt')}")
            .unwrap(),
        json!([1, "Book.TXT", "22"])
    );
    // removed and recreated in another case within one save: the old spelling must not delete the new file
    s.lua_exec("ctx.files:remove('book.txt') ctx.files:write('BOOK.txt', '3')")
        .unwrap();
    drop(s);
    let names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["BOOK.txt"]);
    assert_eq!(fs::read(dir.join("BOOK.txt")).unwrap(), b"3");
}

#[test]
fn fixtures_written_or_edited_during_a_run_answer_the_next_request() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("net-app");
    fs::create_dir_all(&app).unwrap();
    fs::write(
        app.join("manifest.json"),
        r#"{"app_id": "netapp", "entry": "index.lua", "api": "2.26", "permissions": ["net"]}"#,
    )
    .unwrap();
    fs::write(
        app.join("index.lua"),
        "got = 'idle'\nfunction ask() ctx.net:request({url = 'https://x.test/a', method = 'GET'}, \
         function(_, r) got = tostring(r.status) .. ':' .. tostring(r.body or r.err) end) end\n",
    )
    .unwrap();
    let mut s = Session::open(&app, opts(tmp.path())).unwrap();
    let fixture = |body: &str| {
        let net = tmp.path().join("fixtures/net");
        fs::create_dir_all(&net).unwrap();
        let doc = json!({"request": {"method": "GET", "url": "https://x.test/a"},
                         "response": {"status": 200, "body": body}});
        fs::write(net.join("a.json"), doc.to_string()).unwrap();
    };
    let ask = |s: &mut Session| {
        s.lua_exec("ask()").unwrap();
        s.wait(1000).unwrap();
        s.lua_eval("return got").unwrap()
    };
    assert_eq!(ask(&mut s), json!("nil:no_fixture"));
    fixture("first");
    assert_eq!(
        ask(&mut s),
        json!("200:first"),
        "written after the start, as 0.1 read net/ per request"
    );
    fixture("second answer");
    assert_eq!(
        ask(&mut s),
        json!("200:second answer"),
        "edited during the run"
    );
}
