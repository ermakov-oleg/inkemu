use std::collections::BTreeMap;

use inkcore::{AppFiles, Config, Core, Persisted};

fn app(index_lua: &str, api: &str) -> AppFiles {
    let manifest = format!(
        r#"{{"app_id": "testapp", "display_name": "Test", "entry": "index.lua", "api": "{api}", "permissions": []}}"#
    );
    AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.into_bytes()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]))
}

fn boot(index_lua: &str, api: &str) -> Core {
    let cfg = Config {
        start_local_ms: inkcore::clock::local_ms(2026, 11, 20, 9, 0, 0, 0),
        ..Config::default()
    };
    Core::new(app(index_lua, api), cfg, Persisted::default())
}

#[test]
fn on_load_runs_at_boot() {
    let mut core = boot("function on_load(c) loaded = (c == ctx) end\n", "2.2");
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
    assert_eq!(
        core.lua_eval("return loaded").unwrap(),
        serde_json::json!(true)
    );
    let st = core.status();
    assert!(st.running);
    assert_eq!(st.app_id, "testapp");
    assert_eq!(st.time, "2026-11-20T09:00:00.000");
    assert!(
        core.log()
            .iter()
            .any(|l| l == "09:00:00.000 app testapp started"),
        "{:?}",
        &*core.log()
    );
}

#[test]
fn lua_error_in_on_draw_has_traceback_with_app_lines() {
    let core = boot(
        "local function boom() error('kaput') end\n\
         function on_draw(c, g) g:clear(0); g:text(0, 0, 'before'); boom() end\n",
        "2.2",
    );
    assert_eq!(core.errors().len(), 1, "{:?}", &*core.errors());
    let err = &core.errors()[0];
    assert!(
        err.starts_with("Lua error in on_draw: index.lua:1: kaput"),
        "{err}"
    );
    assert!(
        err.contains("index.lua:2:") && err.contains("stack traceback"),
        "{err}"
    );
    assert!(!err.contains(".rs:"), "{err}");
    assert!(!core.status().running);
}

#[test]
fn api_above_firmware_refused() {
    let core = boot("ran = true\n", "9.0");
    assert_eq!(core.errors().len(), 1);
    let err = &core.errors()[0];
    assert!(err.contains("app needs firmware with API 9.0"), "{err}");
    assert_eq!(
        err,
        "manifest.json: app needs firmware with API 9.0; the emulated firmware has 2.26"
    );
    assert!(!core.status().running);
}

#[test]
fn wait_advances_virtual_time() {
    let mut core = boot("", "2.2");
    core.wait(61_000).unwrap();
    assert_eq!(core.clock_ms(), 61_000);
    assert_eq!(core.now_local(), (2026, 11, 20, 9, 1, 1, 0));
    assert_eq!(core.status().time, "2026-11-20T09:01:01.000");
    assert_eq!(
        core.wait(-1).unwrap_err(),
        "wait() needs a non-negative duration"
    );
}

#[test]
fn leave_and_reload_hand_out_the_state_to_persist() {
    let mut core = boot("function on_load(c) c.state.n = 5 end\n", "2.2");
    assert!(core.take_persist().is_none());
    core.leave();
    let saved = core.take_persist().expect("state saved on leave");
    assert_eq!(saved.state_json.as_deref(), Some(br#"{"n":5}"#.as_slice()));
    assert!(core.take_persist().is_none());

    core.set_app(app(
        "function on_load(c) c.state.n = c.state.n + 1 end\n",
        "2.2",
    ));
    core.reload();
    assert_eq!(
        core.lua_eval("return ctx.state.n").unwrap(),
        serde_json::json!(6)
    );
}

#[test]
fn instruction_budget_survives_a_reload() {
    let cfg = Config {
        instruction_budget: 1_000_000,
        ..Config::default()
    };
    let mut core = Core::new(
        app("function on_load(c) while true do end end\n", "2.2"),
        cfg,
        Persisted::default(),
    );
    core.reload();
    let errors = core.errors().to_vec();
    assert_eq!(errors.len(), 2, "{errors:?}");
    for e in &errors {
        assert!(
            e.contains(
                "index.lua:1: callback exceeded the instruction budget (1000000 instructions)"
            ),
            "{e}"
        );
    }
}
