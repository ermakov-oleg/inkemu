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

#[test]
fn a_fetcher_answers_every_live_wake_within_one_wait() {
    let lock = "function on_load(c)\n\
          c.net:request({url = 'http://x.test/' .. c.clock:millis()}, function(c, r) got = r.body end)\n\
        end\n\
        function on_draw(c, g) g:text(0, 0, c.lock.reason .. ' ' .. tostring(got)) end\n";
    let cfg = Config {
        net_mode: NetMode::Live,
        ..Config::default()
    };
    let fetcher: inkcore::Fetcher = Box::new(|r| inkcore::Fetched {
        result: NetResult::Response {
            status: 200,
            headers: vec![],
            body: r.url.rsplit('/').next().unwrap().as_bytes().to_vec(),
            delay_ms: 300,
        },
        notes: vec![inkcore::NetNote::Log(format!("fetched {}", r.url))],
    });
    let mut core = Core::with_fetcher(
        files(r#"{"interval_sec": 60, "net": true}"#, lock),
        cfg,
        Persisted::default(),
        Some(fetcher),
    );
    core.lock();
    core.wait(180_300).unwrap();
    assert_eq!(core.lock_wakes(), 3);
    assert!(has_text(&core, "timer 180000"), "{:?}", core.texts());
    let fetched: Vec<String> = core
        .log()
        .iter()
        .filter_map(|l| l.split_once("fetched ").map(|(_, u)| u.to_string()))
        .collect();
    assert_eq!(
        fetched,
        ["0", "60000", "120000", "180000"].map(|t| format!("http://x.test/{t}"))
    );
    assert!(core.take_outgoing().is_empty());
}

#[test]
fn lock_screen_frames_are_recorded_like_app_frames() {
    let lock = "function on_draw(c, g) g:text(0, 0, 'locked') end\n";
    let mut core = Core::new(
        files(r#"{"interval_sec": 60}"#, lock),
        Config::default(),
        Persisted::default(),
    );
    core.lock();
    core.unlock();
    assert_eq!(core.status().refreshes, ["full", "full", "full"]);
}

fn app_with(index_lua: &str) -> Core {
    let manifest = r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.33",
        "lockscreen": {"interval_sec": 60}}"#;
    let lock = "function on_draw(c, g) g:text(0, 0, 'locked') end\n";
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
        ("lockscreen.lua".to_string(), lock.as_bytes().to_vec()),
    ]));
    Core::new(files, Config::default(), Persisted::default())
}

fn logged(core: &Core, s: &str) -> Option<usize> {
    core.log().iter().position(|l| l.contains(s))
}

#[test]
fn act_sleep_locks_once_the_up_event_on_input_returns() {
    let mut core = app_with(
        "function on_draw(c, g) g:text(0, 0, 'app screen') end\n\
         function on_input(c, ev)\n\
           if ev.type == 'key' and ev.phase == 'up' then\n\
             c:log('act ' .. select('#', c.system:act('sleep')))\n\
             c:log('after act')\n\
           end\n\
           return true\n\
         end\n\
         function on_leave(c) c:log('on_leave ran') end\n",
    );
    core.key("ok").unwrap();
    let after = logged(&core, "after act").expect("on_input went on after act");
    let left = logged(&core, "on_leave ran").expect("on_leave ran");
    assert!(after < left);
    assert!(logged(&core, "act 0").is_some());
    assert!(core.locked() && has_text(&core, "locked"));
    core.unlock();
    assert!(!core.locked() && has_text(&core, "app screen"));
}

#[test]
fn act_sleep_and_power_off_are_inactive_outside_an_up_event() {
    let mut core = app_with(
        "function on_input(c, ev)\n\
           if ev.phase ~= 'up' then\n\
             local _, e1 = c.system:act('sleep')\n\
             local _, e2 = c.system:act('power_off')\n\
             c:log(ev.type .. ' ' .. ev.phase .. ' ' .. tostring(e1) .. ' ' .. tostring(e2))\n\
           end\n\
           return true\n\
         end\n",
    );
    core.hold(100, 100, 700).unwrap();
    core.drag(&[(100, 100), (200, 100)], 100).unwrap();
    core.key("ok").unwrap();
    for ev in ["pointer down", "pointer hold", "pointer move", "key down"] {
        assert!(
            logged(&core, &format!("{ev} inactive inactive")).is_some(),
            "{ev}: {:?}",
            core.log().to_vec()
        );
    }
    assert!(!core.locked());
}

#[test]
fn act_sleep_from_an_unhandled_back_locks_without_the_leave_dialog() {
    let mut core = app_with(
        "function on_input(c, ev)\n\
           if ev.key == 'back' and ev.phase == 'up' then c.system:act('sleep') end\n\
           return ev.phase == 'down'\n\
         end\n",
    );
    core.key("back").unwrap();
    assert!(core.locked(), "{:?}", core.log().to_vec());
    assert!(core.status().pending.is_none());
}

#[test]
fn act_sleep_does_not_lock_when_the_callback_fails() {
    let mut core = app_with(
        "function on_input(c, ev)\n\
           if ev.phase == 'up' then c.system:act('sleep') error('boom') end\n\
           return true\n\
         end\n",
    );
    core.tap(10, 10).unwrap();
    assert!(!core.locked());
    assert_eq!(core.errors().len(), 1);
}

#[test]
fn act_power_off_from_an_up_event_returns_nothing_and_stays() {
    let mut core = app_with(
        "function on_input(c, ev)\n\
           if ev.phase == 'up' then c:log('act ' .. select('#', c.system:act('power_off'))) end\n\
           return true\n\
         end\n",
    );
    core.tap(10, 10).unwrap();
    assert!(logged(&core, "act 0").is_some());
    assert!(logged(&core, "power_off: the confirmation dialog is not emulated").is_some());
    let st = core.status();
    assert!(st.running && !st.locked && st.pending.is_none());
}

#[test]
fn act_sleep_is_inactive_in_a_lock_screen_session() {
    let mut core = app_with("");
    core.lock();
    assert_eq!(
        core.lua_eval("return select(2, ctx.system:act('sleep'))")
            .unwrap(),
        "inactive"
    );
}
