//! `ctx.input:edit` as API 2.33.1 §4 describes it (docked input, emulated by the dialog plate).
use std::collections::BTreeMap;

use inkcore::{AppFiles, Config, Core, Persisted};
use serde_json::json;

fn boot(index_lua: &str) -> Core {
    let manifest =
        r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.33", "permissions": []}"#;
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]));
    let core = Core::new(files, Config::default(), Persisted::default());
    assert!(core.errors().is_empty(), "{:?}", core.errors().to_vec());
    core
}

fn eval(core: &mut Core, code: &str) -> serde_json::Value {
    core.lua_eval(code)
        .unwrap_or_else(|e| panic!("{code}: {e}"))
}

fn pending_kind(core: &Core) -> Option<&'static str> {
    core.status().pending.map(|p| p.kind)
}

const ENDS: &str = "ends = {}\n\
    function on_end(c, s, how) ends[#ends + 1] = s .. ':' .. how end\n";

#[test]
fn text_and_close_are_methods() {
    let mut core = boot(ENDS);
    assert_eq!(
        eval(
            &mut core,
            "ed = ctx.input:edit({text = 'x', on_end = on_end})\n\
             return {type(ed.text), ed:text(), ed:set_text('hello'), ed:text()}"
        ),
        json!(["function", "x", true, "hello"]),
        "H4: API 2.33.1 contract §4 says otherwise: text is a method, set_text returns true"
    );
    assert_eq!(core.status().pending.unwrap().text, "hello");
    assert_eq!(
        eval(&mut core, "return {ed:close(), ed:close() == nil, ed:text() == nil, #ends}"),
        json!(["hello", true, true, 0]),
        "H4: API 2.33.1 contract §4 says otherwise: close returns the final text once and does not call on_end"
    );
    assert_eq!(pending_kind(&core), None);
    assert_eq!(
        eval(
            &mut core,
            "local a, b = ed:set_text('y') return {a == nil, b}"
        ),
        json!([true, "inactive"])
    );
}

#[test]
fn text_and_close_after_on_end_are_nil() {
    let mut core = boot(ENDS);
    eval(&mut core, "ed = ctx.input:edit({on_end = on_end}) return 0");
    core.type_text("ab").unwrap();
    assert_eq!(
        eval(
            &mut core,
            "return {ends[1], ed:text() == nil, ed:close() == nil}"
        ),
        json!(["ab:enter", true, true])
    );
}

#[test]
fn back_ends_editing_with_back() {
    let mut core = boot(&format!(
        "{ENDS}keys = 0\nfunction on_input(c, e) keys = keys + 1 return false end\n\
         function on_load(c) ed = c.input:edit({{text = 'hi', on_end = on_end}}) end\n"
    ));
    assert_eq!(pending_kind(&core), Some("edit"));
    let t0 = core.clock_ms();
    core.key("back").unwrap();
    assert_eq!(core.clock_ms() - t0, 350, "one input");
    assert_eq!(pending_kind(&core), None, "no Leave app? dialog either");
    assert!(core.status().running);
    assert_eq!(
        eval(&mut core, "return {ends, keys}"),
        json!([["hi:back"], 0]),
        "H4: API 2.33.1 contract §4 says otherwise: Back ends editing with how = back, the app does not get it"
    );
}

#[test]
fn cancel_answer_is_back() {
    let mut core = boot(ENDS);
    eval(
        &mut core,
        "ed = ctx.input:edit({text = 'q', on_end = on_end}) return 0",
    );
    core.answer(false).unwrap();
    assert_eq!(
        eval(&mut core, "return ends"),
        json!(["q:back"]),
        "H4: API 2.33.1 contract §4 says otherwise: how is back, not cancel"
    );
}

#[test]
fn bad_options_are_invalid_argument() {
    let mut core = boot("");
    let bad = [
        "rect = {x = 0, y = 0, w = 0, h = 10}",
        "rect = {x = 0, y = 0, w = 10, h = 0}",
        "rect = {x = -1, y = 0, w = 10, h = 10}",
        "rect = {x = 0, y = 0, w = ctx.screen.width + 1, h = 10}",
        "rect = {x = 0, y = ctx.screen.height - 5, w = 10, h = 6}",
        "rect = {x = 0.5, y = 0, w = 10, h = 10}",
        "rect = {x = 1e30, y = 0, w = 1e30, h = 1}",
        "rect = {x = 0, y = 1e30, w = 1, h = 1e30}",
        "rect = {x = math.maxinteger, y = 0, w = math.maxinteger, h = 1}",
        "text = 'a\\nb'",
        "text = 'abcd', max_chars = 3",
        "text = '\\255'",
        "max_chars = 0",
        "max_chars = 513",
        "max_chars = 2.5",
        "placeholder = string.rep('p', 129)",
        "placeholder = '\\255'",
        "kind = 'email'",
        "enter = 'go'",
        "enter = 'newline'",
    ];
    for opts in bad {
        let code = format!("local a, b = ctx.input:edit({{{opts}}}) return {{a == nil, b}}");
        assert_eq!(
            eval(&mut core, &code),
            json!([true, "invalid_argument"]),
            "{opts}"
        );
        assert_eq!(pending_kind(&core), None, "{opts}");
    }
}

#[test]
fn options_of_the_wrong_type_raise() {
    let mut core = boot("");
    let bad = [
        ("rect = 5", "'rect' to 'edit' (table expected, got number)"),
        ("rect = {x = 0, y = 0, w = 10}", "'rect.h'"),
        ("rect = {x = '0', y = 0, w = 1, h = 1}", "'rect.x'"),
        ("text = 5", "'text'"),
        ("placeholder = {}", "'placeholder'"),
        ("max_chars = '5'", "'max_chars'"),
        ("kind = 5", "'kind'"),
        ("enter = true", "'enter'"),
        ("multiline = 'yes'", "'multiline'"),
        ("on_change = 5", "'on_change'"),
        ("on_end = 'x'", "'on_end'"),
        // every type is checked before any range
        ("max_chars = 0, multiline = 'yes'", "'multiline'"),
        ("kind = 'email', on_end = 1", "'on_end'"),
    ];
    for (opts, msg) in bad {
        let code = format!(
            "local ok, err = pcall(ctx.input.edit, ctx.input, {{{opts}}}) return {{ok, tostring(err)}}"
        );
        let got = eval(&mut core, &code);
        assert_eq!(got[0], json!(false), "{opts}");
        assert!(got[1].as_str().unwrap().contains(msg), "{opts}: {got}");
        assert_eq!(pending_kind(&core), None, "{opts}");
    }
}

#[test]
fn good_options_open_the_field() {
    let mut core = boot("");
    let good = [
        "rect = {x = 0, y = 0, w = ctx.screen.width, h = ctx.screen.height}",
        "rect = {x = 32, y = 132, w = 416, h = 40}, placeholder = string.rep('p', 128)",
        "text = 'a\\nb', multiline = true, enter = 'newline'",
        "text = string.rep('я', 512), max_chars = 512",
        "kind = 'number', enter = 'search'",
        "kind = 'secret', enter = 'save'",
        "kind = 'letters', enter = 'next'",
        "kind = 'text', enter = 'done', multiline = false",
        "max_chars = 1.0",
    ];
    for opts in good {
        let code = format!("local ed = ctx.input:edit({{{opts}}}) return ed:close() ~= nil");
        assert_eq!(eval(&mut core, &code), json!(true), "{opts}");
    }
}

#[test]
fn rect_is_logged() {
    let mut core = boot("");
    eval(
        &mut core,
        "ed = ctx.input:edit({rect = {x = 32, y = 132, w = 416, h = 40}}) return 0",
    );
    assert!(
        core.log().iter().any(|l| l.contains("32,132 416x40")),
        "{:?}",
        core.log().to_vec()
    );
}

#[test]
fn busy_while_editing() {
    let mut core = boot("");
    assert_eq!(
        eval(
            &mut core,
            "ed = ctx.input:edit({text = 'first'})\n\
             local r = {}\n\
             local function add(a, b) r[#r + 1] = tostring(a) .. ',' .. tostring(b) end\n\
             add(ctx.input:edit({text = 'second'}))\n\
             add(ctx.input:text({title = 'T'}, function() end))\n\
             add(ctx.dialog:confirm({title = 'T', text = 'x'}, function() end))\n\
             add(ctx.dialog:toast('hi'))\n\
             add(ed:text())\n\
             return r"
        ),
        json!(["nil,busy", "nil,busy", "nil,busy", "nil,nil", "first,nil"]),
        "H4: API 2.33.1 contract §4 says otherwise: a second edit, text input and confirm are busy while editing"
    );
    assert!(core.warnings().is_empty(), "{:?}", core.warnings().to_vec());
    assert_eq!(core.toasts().to_vec(), vec!["hi".to_string()]);
}

#[test]
fn edit_is_busy_while_a_dialog_is_open() {
    for open in [
        "ctx.input:text({title = 'T'}, function() end)",
        "ctx.dialog:confirm({title = 'T', text = 'x'}, function() end)",
    ] {
        let mut core = boot("");
        let code = format!("{open}\nlocal a, b = ctx.input:edit({{}}) return {{a == nil, b}}");
        assert_eq!(eval(&mut core, &code), json!([true, "busy"]), "{open}");
        assert_ne!(pending_kind(&core), Some("edit"));
    }
}

#[test]
fn a_new_edit_can_start_in_on_end() {
    let mut core = boot(
        "function on_load(c) ed = c.input:edit({on_end = function(c, s, how)\n\
           ed2 = c.input:edit({text = s .. '!'}) end}) end\n",
    );
    core.type_text("a").unwrap();
    assert_eq!(pending_kind(&core), Some("edit"));
    assert_eq!(eval(&mut core, "return ed2:text()"), json!("a!"));
}

#[test]
fn typing_stops_at_max_chars() {
    let mut core = boot(&format!(
        "{ENDS}changes = 0\nfunction on_load(c) ed = c.input:edit({{max_chars = 3, on_end = on_end,\n\
           on_change = function() changes = changes + 1 end}}) end\n"
    ));
    core.type_text("abcdef").unwrap();
    assert_eq!(
        eval(&mut core, "return {ends, changes}"),
        json!([["abc:enter"], 3])
    );
}

#[test]
fn a_typed_newline_is_the_confirm_key() {
    let mut core = boot(&format!(
        "{ENDS}function on_load(c) ed = c.input:edit({{on_end = on_end}}) end\n"
    ));
    core.type_text("ab\ncd").unwrap();
    assert_eq!(eval(&mut core, "return ends"), json!(["ab:enter"]));
}

#[test]
fn newline_enter_inserts_a_line_break_and_keeps_editing() {
    let mut core = boot(&format!(
        "{ENDS}function on_load(c) ed = c.input:edit({{multiline = true, enter = 'newline', on_end = on_end}}) end\n"
    ));
    core.type_text("a\nb").unwrap();
    assert_eq!(pending_kind(&core), Some("edit"));
    assert_eq!(eval(&mut core, "return ed:text()"), json!("a\nb"));
    core.answer(true).unwrap();
    assert_eq!(pending_kind(&core), Some("edit"));
    assert_eq!(
        eval(&mut core, "return {ed:text(), #ends}"),
        json!(["a\nb\n", 0])
    );
    core.key("back").unwrap();
    assert_eq!(eval(&mut core, "return ends"), json!(["a\nb\n:back"]));
}

#[test]
fn set_text_follows_the_text_rules_and_does_not_call_on_change() {
    let mut core = boot("");
    assert_eq!(
        eval(
            &mut core,
            "changes = 0\n\
             ed = ctx.input:edit({max_chars = 3, on_change = function() changes = changes + 1 end})\n\
             local r = {}\n\
             for _, v in ipairs({'a\\nb', 'abcd', '\\255', 'abc'}) do\n\
               local a, b = ed:set_text(v) r[#r + 1] = tostring(a) .. ',' .. tostring(b)\n\
             end\n\
             r[#r + 1] = ed:text() r[#r + 1] = changes\n\
             return r"
        ),
        json!([
            "nil,invalid_argument",
            "nil,invalid_argument",
            "nil,invalid_argument",
            "true,nil",
            "abc",
            0
        ])
    );
}

#[test]
fn set_text_of_a_non_string_raises() {
    let mut core = boot("");
    assert_eq!(
        eval(
            &mut core,
            "ed = ctx.input:edit({text = 'k'})\n\
             local ok1, e1 = pcall(function() return ed:set_text() end)\n\
             local ok2, e2 = pcall(function() return ed:set_text({}) end)\n\
             return {ok1, e1:match('string expected') ~= nil, ok2, e2:match('string expected') ~= nil, ed:text()}"
        ),
        json!([false, true, false, true, "k"]),
        "§0.3: a wrong type raises"
    );
}

const SUBMIT: &str = "ends, changes = {}, {}\n\
    function on_end(c, s, how) ends[#ends + 1] = s .. ':' .. how end\n\
    function on_change(c, s) changes[#changes + 1] = s end\n";

#[test]
fn submit_replaces_the_text_and_confirms() {
    let mut core = boot(&format!(
        "{SUBMIT}function on_load(c) ed = c.input:edit({{text = 'hi', on_end = on_end, on_change = on_change}}) end\n"
    ));
    core.submit_text("hi there").unwrap();
    assert_eq!(
        eval(&mut core, "return {ends, changes}"),
        json!([["hi there:enter"], ["hi there"]]),
        "the prefilled text is not doubled"
    );
    let mut core = boot(&format!(
        "{SUBMIT}function on_load(c) ed = c.input:edit({{text = 'hi', on_end = on_end, on_change = on_change}}) end\n"
    ));
    core.submit_text("hi").unwrap();
    assert_eq!(
        eval(&mut core, "return {ends, #changes}"),
        json!([["hi:enter"], 0]),
        "no on_change when nothing changed"
    );
}

#[test]
fn submit_follows_max_chars_and_multiline() {
    let mut core = boot(&format!(
        "{SUBMIT}function on_load(c) ed = c.input:edit({{max_chars = 3, on_end = on_end}}) end\n"
    ));
    core.submit_text("ab\ncd").unwrap();
    assert_eq!(eval(&mut core, "return ends"), json!(["ab:enter"]));
    let mut core = boot(&format!(
        "{SUBMIT}function on_load(c) ed = c.input:edit({{max_chars = 3, on_end = on_end}}) end\n"
    ));
    core.submit_text("abcdef").unwrap();
    assert_eq!(eval(&mut core, "return ends"), json!(["abc:enter"]));
}

#[test]
fn submit_keeps_a_newline_edit_open_with_the_replaced_text() {
    let mut core = boot(&format!(
        "{SUBMIT}function on_load(c) ed = c.input:edit({{text = 'hi', multiline = true, enter = 'newline',\n\
           on_end = on_end, on_change = on_change}}) end\n"
    ));
    core.submit_text("hi\nyou").unwrap();
    core.submit_text("hi\nyou").unwrap();
    assert_eq!(core.status().pending.map(|p| p.kind), Some("edit"));
    assert_eq!(
        eval(&mut core, "return {ed:text(), changes, #ends}"),
        json!(["hi\nyou", ["hi\nyou"], 0])
    );
}

#[test]
fn submit_answers_a_text_input() {
    let mut core = boot(
        "got = 'none'\nfunction on_load(c) c.input:text({text = 'a'}, function(c, s) got = s end) end\n",
    );
    core.submit_text("abc").unwrap();
    assert_eq!(eval(&mut core, "return got"), json!("abc"));
    assert!(core.submit_text("x").is_err(), "nothing is waiting");
}

/// An app that records its input and opens an edit with its text area at 32,132 416x40.
const ROUTED: &str = "ends = {}\ngot = {}\n\
    function on_end(c, s, how) ends[#ends + 1] = s .. ':' .. how end\n\
    function on_input(c, e)\n\
      got[#got + 1] = e.type == 'key' and (e.key .. ':' .. e.phase) or (e.phase .. ':' .. tostring(e.gesture))\n\
      return true\n\
    end\n\
    function open() ed = ctx.input:edit({text = 'hi', on_end = on_end, rect = {x = 32, y = 132, w = 416, h = 40}}) end\n";

fn got(core: &mut Core) -> serde_json::Value {
    match eval(core, "local g = got got = {} return g") {
        serde_json::Value::Object(o) if o.is_empty() => json!([]),
        v => v,
    }
}

#[test]
fn a_pointer_outside_the_text_area_and_keyboard_reaches_the_app() {
    let mut core = boot(ROUTED);
    eval(&mut core, "open() return 0");
    core.tap(240, 300).unwrap();
    assert_eq!(got(&mut core), json!(["down:nil", "up:tap"]));
    assert_eq!(pending_kind(&core), Some("edit"), "the keyboard stays up");
    // just outside the text area, at its right and bottom edges
    core.tap(448, 140).unwrap();
    core.tap(40, 172).unwrap();
    assert_eq!(
        got(&mut core),
        json!(["down:nil", "up:tap", "down:nil", "up:tap"])
    );
}

#[test]
fn a_pointer_in_the_text_area_or_on_the_keyboard_does_not() {
    let mut core = boot(ROUTED);
    eval(&mut core, "open() return 0");
    let t0 = core.clock_ms();
    core.tap(32, 132).unwrap();
    core.tap(447, 171).unwrap();
    core.tap(5, 800 - 436).unwrap();
    core.tap(5, 799).unwrap();
    core.hold(240, 700, 800).unwrap();
    core.double_tap(240, 150).unwrap();
    assert_eq!(got(&mut core), json!([]));
    assert_eq!(pending_kind(&core), Some("edit"));
    assert!(core.clock_ms() - t0 >= 5 * 350, "each one is an input");
    let log = core.log().join("\n");
    assert!(log.contains("input went to the edit's text area (the host moves the cursor), not the app: tap at 32,132"), "{log}");
    assert!(log.contains(
        "input went to the docked keyboard (type text or answer it), not the app: tap at 5,364"
    ));
}

#[test]
fn a_pointer_belongs_to_the_side_where_it_went_down() {
    let mut core = boot(ROUTED);
    eval(&mut core, "open() return 0");
    // down above the keyboard, up on it: all of it is the app's, and the plate's buttons do not answer
    core.drag(&[(240, 300), (240, 500), (160, 520)], 300)
        .unwrap();
    assert_eq!(
        got(&mut core),
        json!(["down:nil", "move:nil", "move:nil", "up:swipe_down"])
    );
    assert_eq!(pending_kind(&core), Some("edit"));
    // down on the keyboard, up above it: none of it
    core.drag(&[(240, 700), (240, 100)], 300).unwrap();
    assert_eq!(got(&mut core), json!([]));
    // down before the edit opens, up while it is open: the app's
    eval(&mut core, "ed:close() return 0");
    core.pointer("down", 240, 150).unwrap();
    eval(&mut core, "open() return 0");
    core.pointer("up", 240, 150).unwrap();
    assert_eq!(got(&mut core), json!(["down:nil", "up:tap"]));
    // down in the text area, the edit closed before up: still not the app's
    core.pointer("down", 240, 150).unwrap();
    eval(&mut core, "ed:close() return 0");
    core.pointer("up", 240, 150).unwrap();
    assert_eq!(got(&mut core), json!([]));
}

#[test]
fn the_keyboard_plate_buttons_answer_a_tap_that_went_down_on_them() {
    let mut core = boot(ROUTED);
    eval(&mut core, "open() return 0");
    core.tap_text("OK").unwrap();
    assert_eq!(pending_kind(&core), None);
    assert_eq!(eval(&mut core, "return ends"), json!(["hi:enter"]));
    assert_eq!(got(&mut core), json!([]));
}

#[test]
fn keys_other_than_back_and_ok_reach_the_app_while_editing() {
    let mut core = boot(ROUTED);
    eval(&mut core, "open() return 0");
    for k in ["up", "down", "left", "right"] {
        core.key(k).unwrap();
    }
    assert_eq!(
        got(&mut core),
        json!([
            "up:down",
            "up:up",
            "down:down",
            "down:up",
            "left:down",
            "left:up",
            "right:down",
            "right:up"
        ])
    );
    assert_eq!(pending_kind(&core), Some("edit"));
    // ok is the confirm key on a touch device
    core.key("ok").unwrap();
    assert_eq!(pending_kind(&core), None);
    assert_eq!(eval(&mut core, "return ends"), json!(["hi:enter"]));
    assert_eq!(got(&mut core), json!([]));
}

#[test]
fn ok_inserts_a_line_break_in_a_newline_edit() {
    let mut core = boot(&format!(
        "{ENDS}function on_load(c) ed = c.input:edit({{text = 'a', multiline = true, enter = 'newline', on_end = on_end}}) end\n"
    ));
    core.key("ok").unwrap();
    assert_eq!(pending_kind(&core), Some("edit"));
    assert_eq!(eval(&mut core, "return ed:text()"), json!("a\n"));
}

#[test]
fn a_key_up_follows_its_down_into_the_app() {
    let mut core = boot(&format!(
        "{ROUTED}function on_input(c, e)\n\
           got[#got + 1] = e.key .. ':' .. e.phase\n\
           if e.phase == 'down' then open() end\n\
           return true\n\
         end\n"
    ));
    core.key("ok").unwrap();
    assert_eq!(got(&mut core), json!(["ok:down", "ok:up"]));
    assert_eq!(pending_kind(&core), Some("edit"));
}

#[test]
fn other_dialogs_still_take_every_input() {
    let mut core = boot(ROUTED);
    eval(
        &mut core,
        "ctx.input:text({title = 'Name'}, function() end) return 0",
    );
    core.tap(240, 100).unwrap();
    core.key("up").unwrap();
    assert_eq!(got(&mut core), json!([]));
    assert_eq!(pending_kind(&core), Some("text"));
}

#[test]
fn a_wrong_type_on_the_next_field_fails_the_app_from_on_end() {
    let mut core = boot(
        "function on_load(c) ui = c.ui:create() end\n\
         function on_draw(c, g)\n\
           g:clear(0)\n\
           ui:begin_frame(g)\n\
           ui:field('a', {x = 24, y = 40, w = 432, h = 56, enter = 'next'})\n\
           ui:field('b', {x = 24, y = 110, w = 432, h = 56, multiline = 'yes'})\n\
           ui:end_frame()\n\
         end\n\
         function on_input(c, e) return ui:handle_event(e) ~= nil end\n",
    );
    core.tap(100, 60).unwrap();
    assert_eq!(pending_kind(&core), Some("edit"));
    core.key("ok").unwrap();
    let errors = core.errors().to_vec();
    assert!(
        errors.iter().any(|e| e.contains("'multiline'")),
        "{errors:?}"
    );
    assert!(!core.status().running);
}
