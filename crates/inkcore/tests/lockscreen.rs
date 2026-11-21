use std::collections::BTreeMap;

use inkcore::{AppFiles, Config, Core, NetMode, NetResult, Persisted};

const APP: &str = "function on_draw(c, g) g:text(0, 0, 'app screen') end\n";

fn files(block: &str, lock_lua: &str) -> AppFiles {
    let manifest = format!(
        r#"{{"app_id": "testapp", "entry": "index.lua", "api": "2.2", "lockscreen": {block}}}"#
    );
    AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.into_bytes()),
        ("index.lua".to_string(), APP.as_bytes().to_vec()),
        ("lockscreen.lua".to_string(), lock_lua.as_bytes().to_vec()),
    ]))
}

fn has_text(core: &Core, s: &str) -> bool {
    core.texts().iter().any(|t| t.text.contains(s))
}

#[test]
fn a_wake_inside_one_wait_runs_the_timers_of_the_new_state() {
    let lock = "function on_load(c)\n\
          c:after(1000, function() fired = c.lock.reason .. ' ' .. c.clock:millis() end)\n\
        end\n\
        function on_draw(c, g) g:text(0, 0, 'reason ' .. c.lock.reason) end\n";
    let mut core = Core::new(
        files(r#"{"interval_sec": 60}"#, lock),
        Config::default(),
        Persisted::default(),
    );
    core.lock();
    core.wait(61_500).unwrap();
    assert_eq!(core.lock_wakes(), 1);
    assert_eq!(core.lua_eval("return fired").unwrap(), "timer 61000");
    assert!(has_text(&core, "reason timer"));
}

#[test]
fn a_live_wake_waits_for_its_own_answer_and_drops_the_old_one() {
    let lock = "function on_load(c)\n\
          c.net:request({url = 'http://x.test/a'}, function(c, r) got = r.body end)\n\
        end\n\
        function on_draw(c, g) g:text(0, 0, c.lock.reason .. ' ' .. tostring(got)) end\n";
    let cfg = Config {
        net_mode: NetMode::Live,
        ..Config::default()
    };
    let mut core = Core::new(
        files(r#"{"interval_sec": 60, "net": true}"#, lock),
        cfg,
        Persisted::default(),
    );
    core.lock();
    assert!(has_text(&core, "lock nil"));
    let first = core.take_outgoing();
    core.wait(60_000).unwrap();
    let second = core.take_outgoing();
    assert_eq!((first.len(), second.len(), core.net_inflight()), (1, 1, 1));
    let ok = |body: &str| NetResult::Response {
        status: 200,
        headers: vec![],
        body: body.as_bytes().to_vec(),
        delay_ms: 300,
    };
    // the answer to the replaced state's request never reaches the new one
    core.deliver(first[0].id, ok("old"));
    core.wait(1000).unwrap();
    assert!(has_text(&core, "lock nil") && !has_text(&core, "timer"));
    // due 300 ms after the request, so it fires at once and the waiting wake draws
    core.deliver(second[0].id, ok("new"));
    assert!(has_text(&core, "timer new"), "{:?}", core.texts());
    assert_eq!(core.net_inflight(), 0);
}

#[test]
fn leave_while_locked_drops_the_lock_screen() {
    let lock = "function on_draw(c, g) g:text(0, 0, 'locked') end\n";
    let mut core = Core::new(
        files(r#"{"interval_sec": 60}"#, lock),
        Config::default(),
        Persisted::default(),
    );
    core.lock();
    core.leave();
    let st = core.status();
    assert!(!st.locked && !st.running);
    assert!(core
        .log()
        .last()
        .unwrap()
        .ends_with("left while locked: the lock screen was dropped"));
    core.wait(180_000).unwrap();
    assert_eq!(core.lock_wakes(), 0);
    core.reload();
    assert!(has_text(&core, "app screen") && !core.status().locked);
}

#[test]
fn lockscreen_api_keeps_other_keys_of_lock_json() {
    let app =
        "function on_load(c) c.lockscreen:set_wallpaper(); c.lockscreen:clear_default() end\n\
        function on_draw(c, g) g:text(0, 0, 'w ' .. tostring(c.lockscreen:is_wallpaper())) end\n";
    let mut files = files(r#"{"enabled": false}"#, "");
    files.0.insert("index.lua".into(), app.as_bytes().to_vec());
    let persisted = Persisted {
        lock_json: Some(r#"{"z": "é"}"#.as_bytes().to_vec()),
        ..Persisted::default()
    };
    let mut core = Core::new(files, Config::default(), persisted);
    assert!(has_text(&core, "w true"));
    let saved = core.take_persist().unwrap().lock_json.unwrap();
    assert_eq!(
        String::from_utf8(saved).unwrap(),
        r#"{"is_default": false, "is_wallpaper": true, "z": "\u00e9"}"#
    );
    let log = core.log().to_vec();
    assert!(log
        .iter()
        .any(|l| l.ends_with("lockscreen: set_wallpaper (auto-confirmed)")));
    assert!(log.iter().any(|l| l.ends_with("lockscreen: clear_default")));
}
