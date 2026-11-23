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
        "rect = {x = 0, y = 0, w = 10}",
        "rect = 5",
        "text = 'a\\nb'",
        "text = 'abcd', max_chars = 3",
        "text = '\\255'",
        "text = 5",
        "max_chars = 0",
        "max_chars = 513",
        "max_chars = 2.5",
        "placeholder = string.rep('p', 129)",
        "placeholder = '\\255'",
        "kind = 'email'",
        "enter = 'go'",
        "enter = 'newline'",
        "multiline = 'yes'",
        "on_change = 5",
        "on_end = 'x'",
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
