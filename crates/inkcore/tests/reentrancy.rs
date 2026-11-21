//! Callbacks that re-enter the host: each must end as in 0.1, never with a panic or a `BorrowMutError`.
use std::collections::BTreeMap;

use inkcore::{AppFiles, Config, Core, Persisted};

fn boot(index_lua: &str) -> Core {
    let manifest = r#"{"app_id": "testapp", "display_name": "Test", "entry": "index.lua", "api": "2.2", "permissions": []}"#;
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]));
    let cfg = Config {
        start_local_ms: inkcore::clock::local_ms(2026, 11, 20, 9, 0, 0, 0),
        ..Config::default()
    };
    Core::new(files, cfg, Persisted::default())
}

fn has_text(core: &Core, s: &str) -> bool {
    core.texts().iter().any(|t| t.text.contains(s))
}

#[test]
fn a_dialog_callback_opens_another_dialog() {
    let mut core = boot(
        "res = 'none'\n\
         function on_load(c)\n\
           c.dialog:confirm({title = 'A'}, function(c, ok)\n\
             c.dialog:confirm({title = 'B'}, function(c, ok2) res = tostring(ok) .. ':' .. tostring(ok2); c:invalidate() end)\n\
           end)\n\
         end\n\
         function on_draw(c, g) g:text(0, 0, 'res ' .. res) end\n",
    );
    assert_eq!(core.status().pending.unwrap().title, "A");
    core.answer(true).unwrap();
    let p = core.status().pending.expect("the second dialog is open");
    assert_eq!((p.kind, p.title.as_str()), ("confirm", "B"));
    // the first dialog was answered, not replaced
    assert!(core.warnings().is_empty(), "{:?}", &*core.warnings());
    core.answer(false).unwrap();
    assert!(core.status().pending.is_none());
    assert!(has_text(&core, "res true:false"));
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
}

#[test]
fn after_zero_inside_on_draw_fires_on_the_next_wait() {
    let mut core = boot(
        "fired, armed = 0, false\n\
         function on_draw(c, g)\n\
           if not armed then armed = true; c:after(0, function() fired = fired + 1; c:invalidate() end) end\n\
           g:text(0, 0, 'fired ' .. fired)\n\
         end\n",
    );
    assert!(has_text(&core, "fired 0"));
    core.wait(0).unwrap();
    assert!(has_text(&core, "fired 1"));
    core.wait(1000).unwrap();
    assert!(has_text(&core, "fired 1"));
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
}

#[test]
fn after_zero_rearmed_by_every_on_draw_overflows_instead_of_hanging() {
    let mut core = boot(
        "function on_draw(c, g) c:after(0, function() c:invalidate() end) g:text(0, 0, 'x') end\n",
    );
    core.wait(1000).unwrap();
    assert_eq!(core.errors().len(), 1, "{:?}", &*core.errors());
    assert!(core.errors()[0].contains("same virtual instant"));
    assert!(!core.status().running);
}

#[test]
fn quit_inside_on_draw_leaves_after_the_callback() {
    let core = boot(
        "function on_draw(c, g) c:quit(); g:text(0, 0, 'drawn') end\n\
         function on_leave(c) c:log('left') end\n",
    );
    assert!(!core.status().running);
    let log = core.log().to_vec();
    let left = log.iter().position(|l| l.ends_with("[app] left"));
    let gone = log.iter().position(|l| l.ends_with("app left"));
    assert!(left.is_some() && gone > left, "{log:?}");
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
}

#[test]
fn timer_error_in_the_input_wait_while_a_dialog_is_open() {
    // `key` delivers on_input, then waits 350 ms: the failing timer fires inside that wait, with a dialog open
    // and another timer still armed
    let mut core = boot(
        "msg = 'none'\n\
         function on_input(c, ev)\n\
           if ev.phase == 'up' then\n\
             c:after(100, function() error('late') end)\n\
             c:after(200, function() msg = 'second' end)\n\
             c.dialog:confirm({title = 'Sure?'}, function() msg = 'answered' end)\n\
           end\n\
           return true\n\
         end\n\
         function on_draw(c, g) g:text(0, 0, 'msg ' .. msg) end\n",
    );
    core.key("ok").unwrap();
    assert_eq!(core.errors().len(), 1, "{:?}", &*core.errors());
    let err = core.errors()[0].clone();
    assert!(
        err.starts_with("Lua error in ctx:after callback: index.lua:4: late"),
        "{err}"
    );
    let st = core.status();
    assert!(!st.running && st.pending.is_none());
    core.wait(1000).unwrap();
    assert!(!has_text(&core, "msg second") && has_text(&core, "App error"));
    assert_eq!(
        core.answer(true).unwrap_err(),
        "no dialog is waiting for an answer"
    );
}
