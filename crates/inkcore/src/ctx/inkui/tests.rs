use std::collections::BTreeMap;

use mlua::{Function, Lua, Table};
use serde_json::json;

use super::{api_minor, host, load, namespace, FILES};
use crate::app::AppFiles;
use crate::lua::{call_guarded, load_chunk, new_lua, set_budget};
use crate::{Config, Core, Persisted};

fn app_files(manifest: &str, index_lua: &str) -> AppFiles {
    AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]))
}

fn boot(index_lua: &str) -> Core {
    boot_api("2.33", index_lua)
}

fn boot_api(api: &str, index_lua: &str) -> Core {
    let manifest = format!(r#"{{"app_id": "testapp", "entry": "index.lua", "api": "{api}"}}"#);
    let core = Core::new(
        app_files(&manifest, index_lua),
        Config::default(),
        Persisted::default(),
    );
    assert!(core.errors().is_empty(), "{:?}", core.errors().to_vec());
    core
}

fn eval(core: &mut Core, code: &str) -> serde_json::Value {
    core.lua_eval(code)
        .unwrap_or_else(|e| panic!("{code}: {e}"))
}

#[test]
fn ctx_ui_is_an_object_with_only_create() {
    let mut core = boot("");
    assert_eq!(
        eval(
            &mut core,
            "local keys = {} for k in pairs(ctx.ui) do keys[#keys + 1] = k end\n\
             return {ctx.capabilities.ui, type(ctx.ui), type(ctx.ui.create), #keys, keys[1]}"
        ),
        json!([true, "table", "function", 1, "create"])
    );
}

#[test]
fn create_returns_independent_objects_with_hidden_state() {
    let mut core = boot("");
    assert_eq!(
        eval(
            &mut core,
            "local a, b = ctx.ui:create(), ctx.ui:create({})\n\
             local c = ctx.ui:create({unknown = 1, pressed = true, focus_style = 'outline'})\n\
             return {type(a), type(b), type(c), a ~= b, getmetatable(a), next(a) == nil,\n\
               type(a.begin_frame), type(a.handle_event), type(a.grid), a.objects == nil, a.state_of == nil}"
        ),
        json!(["table", "table", "table", true, false, true, "function", "function", "function", true, true])
    );
}

#[test]
fn create_refuses_what_is_not_an_options_table() {
    let mut core = boot("");
    for opts in [
        "5",
        "'x'",
        "true",
        "false",
        "{focus_style = 'bold'}",
        "{focus_style = 1}",
        "{focus_style = false}",
    ] {
        assert_eq!(
            eval(
                &mut core,
                &format!("local r, e = ctx.ui:create({opts}) return {{r == nil, e}}")
            ),
            json!([true, "invalid_argument"]),
            "{opts}"
        );
    }
}

#[test]
fn focus_style_is_a_2_16_field() {
    let create = "local r, e = ctx.ui:create({focus_style = 'bold'}) return {type(r), tostring(e)}";
    let mut core = boot_api("2.15", "");
    assert_eq!(eval(&mut core, create), json!(["table", "nil"]));
    let mut core = boot_api("2.16", "");
    assert_eq!(eval(&mut core, create), json!(["nil", "invalid_argument"]));
}

#[test]
fn every_fresh_lua_state_gets_the_library() {
    let mut core = boot("");
    let create = "local ui = ctx.ui:create() return {type(ui), type(ui.begin_frame)}";
    core.reload();
    assert_eq!(eval(&mut core, create), json!(["table", "function"]));
    core.restart();
    assert_eq!(eval(&mut core, create), json!(["table", "function"]));
}

#[test]
fn dot_calls_and_stubs_raise_at_the_app_line() {
    let mut core = boot("");
    let err = core.lua_exec("ctx.ui.create()").unwrap_err();
    assert!(
        err.starts_with(
            "lua_exec:1: bad argument #2 to 'create' (call ctx.ui:create with a colon)"
        ),
        "{err}"
    );
    let err = core
        .lua_exec("local ui = ctx.ui:create()\nui.begin_frame()")
        .unwrap_err();
    assert!(
        err.starts_with(
            "lua_exec:2: bad argument #1 to 'begin_frame' (call ui:begin_frame with a colon)"
        ),
        "{err}"
    );
    let err = core
        .lua_exec("local ui = ctx.ui:create()\nui:button('ok', {})")
        .unwrap_err();
    assert!(
        err.starts_with("lua_exec:2: ui:button is not implemented in inkemu yet"),
        "{err}"
    );
}

#[test]
fn a_library_error_in_a_callback_is_an_app_error() {
    let core = Core::new(
        app_files(
            r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.33"}"#,
            "local ui = ctx.ui:create()\nfunction on_draw(ctx, g)\n  ui:begin_frame(g)\nend",
        ),
        Config::default(),
        Persisted::default(),
    );
    let errors = core.errors().to_vec();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("index.lua:3: ui:begin_frame is not implemented in inkemu yet")),
        "{errors:?}"
    );
}

#[test]
fn app_replaced_base_functions_do_not_reach_the_library() {
    let mut core = boot(
        "type = function() return 'table' end\n\
         setmetatable, error = nil, nil",
    );
    assert_eq!(
        eval(
            &mut core,
            "local r, e = ctx.ui:create(5) local ui = ctx.ui:create()\n\
             return {r == nil, e, rawequal(ui, nil), next(ui) == nil}"
        ),
        json!([true, "invalid_argument", false, true])
    );
}

/// A bare Lua state with the library and `extra` loaded; `probe` is the last extra file's environment view.
fn library(api: &str, extra: &'static str) -> (Lua, Table) {
    let lua = new_lua();
    let ctx = lua.create_table().unwrap();
    let manifest = format!(r#"{{"app_id": "testapp", "api": "{api}"}}"#);
    let host = host(&lua, &ctx, &app_files(&manifest, "")).unwrap();
    let mut files = FILES.to_vec();
    files.push(("probe.lua", extra));
    let env = load(&lua, &files, host).unwrap();
    (lua, env)
}

fn run(lua: &Lua, code: &str) -> Result<mlua::MultiValue, String> {
    let f = load_chunk(lua, code.as_bytes(), "@index.lua").unwrap();
    call_guarded(lua, &f, ())
}

#[test]
fn the_library_sees_only_its_own_environment() {
    let (lua, env) = library(
        "2.33",
        "function probe() return SECRET, ctx, require, coroutine, _G end",
    );
    lua.globals().set("SECRET", 1).unwrap();
    lua.globals()
        .set("ctx", lua.create_table().unwrap())
        .unwrap();
    lua.globals()
        .set("probe", env.get::<Function>("probe").unwrap())
        .unwrap();
    let seen = run(&lua, "return select('#', probe()), probe()").unwrap();
    let mut seen = seen.into_iter();
    assert_eq!(seen.next().unwrap().as_i32(), Some(5));
    assert!(seen.all(|v| v.is_nil()));
}

#[test]
fn the_library_has_its_own_copies_of_the_standard_tables() {
    let (lua, env) = library(
        "2.33",
        "function probe(app)\n\
           return string.rep ~= nil and table.insert ~= nil and math.floor ~= nil and utf8.char ~= nil,\n\
             rawequal(string, app[1]) or rawequal(table, app[2]) or rawequal(math, app[3])\n\
               or rawequal(utf8, app[4])\n\
         end",
    );
    lua.globals()
        .set("probe", env.get::<Function>("probe").unwrap())
        .unwrap();
    let r = run(
        &lua,
        "local app = {string, table, math, utf8}\n\
         string.rep, table.insert, math.floor, utf8.char = nil, nil, nil, nil\n\
         return probe(app)",
    )
    .unwrap();
    let r: Vec<bool> = r.into_iter().map(|v| v.as_boolean().unwrap()).collect();
    assert_eq!(r, [true, false]);
}

#[test]
fn the_library_gets_the_package_minor_not_the_firmware_one() {
    let (lua, env) = library(
        "2.13",
        "function probe() return host.api_minor, api_at_least(13), api_at_least(14), host.pointer end",
    );
    lua.globals()
        .set("probe", env.get::<Function>("probe").unwrap())
        .unwrap();
    let r: (i32, bool, bool, bool) = lua.load("return probe()").eval().unwrap();
    assert_eq!(r, (13, true, false, true));
}

#[test]
fn api_minor_reads_the_manifest() {
    let minor = |api: &str| api_minor(&app_files(&format!(r#"{{"app_id": "testapp"{api}}}"#), ""));
    assert_eq!(minor(r#", "api": "2.15""#), 15);
    assert_eq!(minor(r#", "api": " 2.9 ""#), 9);
    assert_eq!(minor(r#", "api": "2.013""#), 13);
    assert_eq!(minor(r#", "api": 2.16"#), 16);
    assert_eq!(minor(r#", "api": "1.40""#), 0);
    assert_eq!(minor(""), 0);
}

#[test]
fn library_code_runs_under_the_app_budget() {
    let (lua, env) = library(
        "2.33",
        "function spin() while true do end end\n\
         function call(f) return f() end\n\
         create = function() while true do end end",
    );
    set_budget(&lua, 1_000_000);
    for name in ["spin", "call"] {
        lua.globals()
            .set(name, env.get::<Function>(name).unwrap())
            .unwrap();
    }
    lua.globals()
        .set("ui", namespace(&lua, &env).unwrap())
        .unwrap();
    let budget = "callback exceeded the instruction budget (1000000 instructions) - infinite loop?";
    for code in [
        "spin()",
        "pcall(spin) return 'swallowed'",
        "pcall(ui.create, ui) return 'swallowed'",
    ] {
        let err = run(&lua, code).unwrap_err();
        assert!(err.contains(budget), "{code}: {err}");
    }
    // a callback the library calls back into, as a `region`'s `draw`
    let err = run(&lua, "call(function() while true do end end)").unwrap_err();
    assert!(err.starts_with(&format!("index.lua:1: {budget}")), "{err}");
}
