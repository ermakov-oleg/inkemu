use std::collections::BTreeMap;

use inkcore::{AppFiles, Config, Core, Persisted};
use serde_json::json;

const BIG_WARN: &str =
    "JSON number 1700000000000 does not fit a 32-bit integer; on the device it loses precision";

fn app(index_lua: &str) -> AppFiles {
    let manifest =
        r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.2", "permissions": []}"#;
    AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]))
}

fn boot_with(index_lua: &str, state_json: Option<&[u8]>) -> Core {
    let cfg = Config {
        start_local_ms: inkcore::clock::local_ms(2026, 11, 20, 9, 0, 0, 0),
        utc_offset_sec: 3600,
        ..Config::default()
    };
    let persisted = Persisted {
        state_json: state_json.map(<[u8]>::to_vec),
        ..Persisted::default()
    };
    Core::new(app(index_lua), cfg, persisted)
}

fn boot(index_lua: &str) -> Core {
    boot_with(index_lua, None)
}

fn saved_state(core: &mut Core) -> String {
    core.leave();
    let p = core.take_persist().expect("state was saved");
    String::from_utf8(p.state_json.expect("state.json")).unwrap()
}

#[test]
fn nested_tables_round_trip_through_ctx_json() {
    let mut core = boot("");
    let got = core
        .lua_eval(
            "local s = ctx.json:encode({a = {1, 2, {x = 'y'}}, b = true, n = 1.5, t = 'т'})\n\
             local back = ctx.json:decode(s)\n\
             return {s, back, ctx.json:encode(back) == s}",
        )
        .unwrap();
    let s = r#"{"a":[1,2,{"x":"y"}],"b":true,"n":1.5,"t":"т"}"#;
    assert_eq!(
        got,
        json!([s, {"a": [1, 2, {"x": "y"}], "b": true, "n": 1.5, "t": "т"}, true])
    );
}

#[test]
fn state_round_trips_and_is_written_as_0_1_did() {
    let mut core = boot(
        "function on_load(c) c.state.n = (c.state.n or 0) + 1; c.state.list = {1, 2.5, 'три'}; \
         c.state.flag = true; c.state.f = 0.1 end",
    );
    let first = saved_state(&mut core);
    assert_eq!(
        first,
        r#"{"f": 0.1, "flag": true, "list": [1, 2.5, "три"], "n": 1}"#
    );
    let mut again = boot_with(
        "function on_load(c) c.state.n = c.state.n + 1 end",
        Some(first.as_bytes()),
    );
    assert_eq!(
        again
            .lua_eval("return math.type(ctx.state.list[2])")
            .unwrap(),
        "float"
    );
    assert_eq!(
        saved_state(&mut again),
        r#"{"f": 0.1, "flag": true, "list": [1, 2.5, "три"], "n": 2}"#
    );
    assert!(again.warnings().is_empty(), "{:?}", &*again.warnings());
}

#[test]
fn json_null_is_a_sentinel() {
    let mut core = boot("");
    let got = core
        .lua_eval(
            "local v = ctx.json:decode('{\"a\":null,\"l\":[1,null,3]}')\n\
             return {v.a == ctx.json.null, v.l[2] == ctx.json.null, #v.l, tostring(ctx.json.null),\n\
               getmetatable(ctx.json.null), ctx.json:encode({a = ctx.json.null}), ctx.json:encode(ctx.json.null)}",
        )
        .unwrap();
    assert_eq!(
        got,
        json!([true, true, 3, "null", false, r#"{"a":null}"#, "null"])
    );
}

#[test]
fn binary_strings_cannot_be_encoded() {
    let mut core = boot("");
    let got = core
        .lua_eval(
            "local a, e = ctx.json:encode('\\0\\255')\n\
             local nested = ctx.json:encode({ok = '\\0', bad = '\\0\\255'})\n\
             return {tostring(a), e, nested}",
        )
        .unwrap();
    assert_eq!(
        got,
        json!(["nil", "invalid_argument", r#"{"ok":"\u0000"}"#])
    );
    let warnings = core.warnings().to_vec();
    assert_eq!(
        warnings,
        ["json.bad: a string that is not valid UTF-8 cannot be saved, skipped"]
    );
}

#[test]
fn a_json_integer_beyond_int32_is_a_float_with_one_warning_per_callback() {
    let mut core = boot("");
    let got = core
        .lua_eval(
            "local v = ctx.json:decode('{\"t\":1700000000000, \"u\":1700000000000, \"s\":5}')\n\
             return {math.type(v.t), v.t == 1700000000000.0, math.type(v.s)}",
        )
        .unwrap();
    assert_eq!(got, json!(["float", true, "integer"]));
    assert_eq!(core.warnings().to_vec(), [BIG_WARN]);
    core.lua_exec("ctx.json:decode('[1700000000000]')").unwrap();
    assert_eq!(core.warnings().to_vec(), [BIG_WARN, BIG_WARN]);
}

#[test]
fn clock_now_has_integer_epoch_and_local_seconds() {
    let mut core = boot("");
    let got = core
        .lua_eval(
            "local t = ctx.clock:now()\n\
             return {math.type(t.epoch_sec), math.type(t.local_sec), t.hour, t.local_sec - t.epoch_sec,\n\
               t.wday, t.yday, t.valid, ctx.clock:millis()}",
        )
        .unwrap();
    assert_eq!(got, json!(["integer", "integer", 9, 3600, 6, 324, true, 0]));
    let epoch = core.lua_eval("return ctx.clock:now().epoch_sec").unwrap();
    // 2026-11-20T09:00 at UTC+1 is 08:00 UTC
    assert_eq!(epoch, json!(1_795_161_600));
    core.wait(61_000).unwrap();
    assert_eq!(core.lua_eval("return ctx.clock:millis()").unwrap(), 61_000);
    assert_eq!(core.lua_eval("return ctx.clock:now().minute").unwrap(), 1);
}

#[test]
fn log_prints_numbers_as_python_did() {
    let core = boot("ctx:log(1.0, 0.5, 1e16, 1e-5, 0.0001, 0.1, -0.0, 3, nil, true, {}, 1/0, 'x')");
    let last = core
        .log()
        .iter()
        .find(|l| l.contains("[app]"))
        .cloned()
        .unwrap();
    assert!(
        last.ends_with("[app] 1.0 0.5 1e+16 1e-05 0.0001 0.1 -0.0 3 nil true table inf x"),
        "{last}"
    );
}

#[test]
fn an_empty_table_from_on_input_handles_the_back_key() {
    let mut core = boot("function on_input(c, ev) return {} end");
    core.key("back").unwrap();
    assert_eq!(core.status().pending, None);
}

#[test]
fn unreadable_or_non_table_state_starts_empty_with_a_warning() {
    for (saved, needle) in [
        (&b"{not json"[..], "state.json: unreadable state ("),
        (
            &b"5"[..],
            "state.json: state is not a table, starting with an empty ctx.state",
        ),
    ] {
        let mut core = boot_with("", Some(saved));
        assert_eq!(
            core.lua_eval("return next(ctx.state) == nil").unwrap(),
            true
        );
        let w = core.warnings().to_vec();
        assert!(w.len() == 1 && w[0].starts_with(needle), "{w:?}");
    }
}

#[test]
fn a_non_table_state_is_not_saved_and_named_a_number() {
    let mut core = boot("function on_load(c) c.state = 5 end");
    core.leave();
    assert_eq!(
        core.warnings().to_vec(),
        ["ctx.state is a number, not a table: not saved"]
    );
}
