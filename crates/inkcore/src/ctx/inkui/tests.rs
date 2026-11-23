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
        .lua_exec("local ui = ctx.ui:create()\nui:switch('ok', {})")
        .unwrap_err();
    assert!(
        err.starts_with("lua_exec:2: ui:switch is not implemented in inkemu yet"),
        "{err}"
    );
}

#[test]
fn a_library_error_in_a_callback_is_an_app_error() {
    let core = Core::new(
        app_files(
            r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.33"}"#,
            "local ui = ctx.ui:create()\nfunction on_draw(ctx, g)\n  ui:handle_event({})\nend",
        ),
        Config::default(),
        Persisted::default(),
    );
    let errors = core.errors().to_vec();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("index.lua:3: ui:handle_event is not implemented in inkemu yet")),
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

/// Fonts with the oracle's fixed pitch (line height 20/24/30, 10/12/15 px per byte) and a `layout` that fits whole
/// lines of `w // advance` bytes, enough to tell which font a label's text fits in.
const FIXED_FONTS: &str = r#"
local function font(name, lh, adv)
  local f = { name = name, line_height = lh }
  function f.measure(self, s) return #tostring(s) * adv end
  function f.layout(self, s, w, h)
    local per, rows = w // adv, h // lh
    local lines = (#s + per - 1) // per
    if lines <= rows then return lines, lines * lh, #s end
    return rows, rows * lh, rows * per
  end
  return f
end
return { small = font("small", 20, 10), medium = font("medium", 24, 12), large = font("large", 30, 15) }
"#;

/// A canvas that writes each call as `op(args)`, option tables with sorted keys and fonts by name.
const RECORDER: &str = r#"
local function show(v)
  if type(v) ~= "table" then return tostring(v) end
  if v.name then return "@" .. v.name end
  local keys = {}
  for k in pairs(v) do keys[#keys + 1] = k end
  table.sort(keys)
  for i, k in ipairs(keys) do keys[i] = k .. "=" .. show(v[k]) end
  return "{" .. table.concat(keys, ",") .. "}"
end
return function(log)
  local g = {}
  for _, op in ipairs({ "rect", "text_box", "pattern", "circle" }) do
    g[op] = function(self, ...)
      local args = table.pack(...)
      for i = 1, args.n do args[i] = show(args[i]) end
      log[#log + 1] = op .. "(" .. table.concat(args, ",", 1, args.n) .. ")"
    end
  end
  return g
end
"#;

/// The library for a package of `api` with [`FIXED_FONTS`], and globals for the app chunk: `make` (create),
/// `state` (an object's private state), `fonts`, `input` (`ctx.input`) and `recorder` ([`RECORDER`]).
fn fixed_library(api: &str) -> Lua {
    let lua = new_lua();
    let ctx = lua.create_table().unwrap();
    let fonts: Table = lua.load(FIXED_FONTS).eval().unwrap();
    ctx.set("fonts", fonts).unwrap();
    ctx.set("input", lua.create_table().unwrap()).unwrap();
    let manifest = format!(r#"{{"app_id": "testapp", "api": "{api}"}}"#);
    let host = host(&lua, &ctx, &app_files(&manifest, "")).unwrap();
    let mut files = FILES.to_vec();
    files.push(("probe.lua", "function state(ui) return objects[ui] end"));
    let env = load(&lua, &files, host).unwrap();
    let g = lua.globals();
    g.set("make", env.get::<Function>("create").unwrap())
        .unwrap();
    g.set("fonts", ctx.get::<Table>("fonts").unwrap()).unwrap();
    g.set("input", ctx.get::<Table>("input").unwrap()).unwrap();
    g.set("state", env.get::<Function>("state").unwrap())
        .unwrap();
    g.set("recorder", lua.load(RECORDER).eval::<Function>().unwrap())
        .unwrap();
    lua
}

/// The canvas calls of one frame declared by `body` (Lua using `ui`), after `setup` (Lua using `ui` and `st`).
fn frame(lua: &Lua, opts: &str, setup: &str, body: &str) -> Vec<String> {
    let code = format!(
        "local ui = make({opts}) local st = state(ui) {setup}\n\
         local log = {{}} ui:begin_frame(recorder(log))\n{body}\nui:end_frame() return log"
    );
    let r = run(lua, &code).unwrap_or_else(|e| panic!("{e}"));
    let log: Table = r.into_iter().next().unwrap().as_table().unwrap().clone();
    log.sequence_values::<String>()
        .map(Result::unwrap)
        .collect()
}

const FOCUS_ROW: &str = "ui:button('n', {x = 10, y = 0, w = 200, h = 56, text = 'B'})\n\
     ui:button('r8', {x = 10, y = 70, w = 200, h = 56, text = 'B', radius = 8})\n\
     ui:button('sel', {x = 10, y = 140, w = 200, h = 56, text = 'B', selected = true})\n\
     ui:button('emp', {x = 10, y = 210, w = 200, h = 56, text = 'B', emphasis = true})\n\
     ui:button('nob', {x = 10, y = 280, w = 200, h = 56, text = 'B', border = false})\n\
     ui:region('reg', {x = 10, y = 350, w = 200, h = 56, draw = function(g, r, st) g:rect(r.x, r.y, r.w, r.h, st) end})";

fn text(y: i32, color: i32) -> String {
    format!(
        "text_box(10,{y},200,56,B,{{align=center,color={color},font=@medium,overflow=ellipsis,valign=middle,wrap=none}})"
    )
}

fn body(y: i32, fill: bool, r: i32) -> String {
    format!("rect(10,{y},200,56,{{fill={fill},radius={r}}})")
}

fn ring(y: i32, r: i32) -> String {
    format!("rect(7,{},206,62,{{line_width=2,radius={r}}})", y - 3)
}

fn region(focused: bool) -> String {
    format!("rect(10,350,200,56,{{enabled=true,focused={focused},selected=false}})")
}

/// The unfocused look of `FOCUS_ROW`, each control's calls in order.
fn plain_row() -> Vec<Vec<String>> {
    vec![
        vec![body(0, false, 0), text(0, 15)],
        vec![body(70, false, 8), text(70, 15)],
        vec![body(140, true, 0), text(140, 0)],
        vec![body(210, true, 0), text(210, 0)],
        vec![text(280, 15)],
        vec![region(false)],
    ]
}

#[test]
fn focus_is_drawn_only_while_keys_drive_it() {
    let lua = fixed_library("2.26");
    let want: Vec<String> = plain_row().concat();
    assert_eq!(frame(&lua, "nil", "st.focus = 'n'", FOCUS_ROW), want);
}

#[test]
fn the_fill_focus_look_fills_and_rings_selected_and_emphasis() {
    let lua = fixed_library("2.26");
    let cases: [(&str, usize, Vec<String>); 6] = [
        ("n", 0, vec![body(0, true, 0), text(0, 0)]),
        ("r8", 1, vec![body(70, true, 8), text(70, 0)]),
        (
            "sel",
            2,
            vec![body(140, true, 0), ring(140, 0), text(140, 0)],
        ),
        (
            "emp",
            3,
            vec![body(210, true, 0), ring(210, 0), text(210, 0)],
        ),
        ("nob", 4, vec![body(280, true, 0), text(280, 0)]),
        ("reg", 5, vec![region(true)]),
    ];
    for (id, at, look) in cases {
        let mut want = plain_row();
        want[at] = look;
        let setup = format!("st.keys = true st.focus = '{id}'");
        assert_eq!(frame(&lua, "nil", &setup, FOCUS_ROW), want.concat(), "{id}");
    }
}

#[test]
fn the_outline_focus_look_only_adds_the_ring() {
    let lua = fixed_library("2.26");
    let cases: [(&str, usize, Vec<String>); 6] = [
        ("n", 0, vec![body(0, false, 0), ring(0, 0), text(0, 15)]),
        (
            "r8",
            1,
            vec![body(70, false, 8), ring(70, 11), text(70, 15)],
        ),
        (
            "sel",
            2,
            vec![body(140, true, 0), ring(140, 0), text(140, 0)],
        ),
        (
            "emp",
            3,
            vec![body(210, true, 0), ring(210, 0), text(210, 0)],
        ),
        ("nob", 4, vec![ring(280, 0), text(280, 15)]),
        ("reg", 5, vec![region(true)]),
    ];
    for (id, at, look) in cases {
        let mut want = plain_row();
        want[at] = look;
        let setup = format!("st.keys = true st.focus = '{id}'");
        assert_eq!(
            frame(&lua, "{focus_style = 'outline'}", &setup, FOCUS_ROW),
            want.concat(),
            "{id}"
        );
    }
}

#[test]
fn the_outline_style_is_a_2_16_feature() {
    let lua = fixed_library("2.15");
    let want = [
        vec![body(0, true, 0), text(0, 0)],
        plain_row()[1..].concat(),
    ]
    .concat();
    assert_eq!(
        frame(
            &lua,
            "{focus_style = 'outline'}",
            "st.keys = true st.focus = 'n'",
            FOCUS_ROW
        ),
        want
    );
}

#[test]
fn focus_settles_at_end_frame() {
    let lua = fixed_library("2.26");
    let code = "local ui = make() local g = recorder({})\n\
        local function f(a_on, b_on)\n\
          ui:begin_frame(g)\n\
          ui:label('l', {x = 0, y = 0, w = 10, h = 10})\n\
          ui:button('a', {x = 0, y = 100, w = 10, h = 10, enabled = a_on})\n\
          ui:button('b', {x = 0, y = 0, w = 10, h = 10, enabled = b_on})\n\
          ui:region('c', {x = 0, y = 120, w = 10, h = 10})\n\
          ui:end_frame()\n\
          return ui:focused()\n\
        end\n\
        local out = {tostring(ui:focused()), f(true, true)}\n\
        state(ui).focus = 'c' out[#out + 1] = f(true, true)\n\
        state(ui).focus = 'l' out[#out + 1] = f(true, true)\n\
        state(ui).focus = 'c' ui:begin_frame(g) ui:label('l', {x = 0, y = 0, w = 1, h = 1}) ui:end_frame()\n\
        out[#out + 1] = tostring(ui:focused())\n\
        state(ui).focus = 'a' out[#out + 1] = f(false, true)\n\
        return table.concat(out, ' ')";
    let r = run(&lua, code).unwrap();
    // absent id or a label -> first available; disabled -> nearest centre (c, 20 px away, before b, 100 px)
    assert_eq!(
        r.into_iter().next().unwrap().to_string().unwrap(),
        "nil a c a nil c"
    );
}

#[test]
fn font_min_takes_the_largest_system_font_that_fits() {
    let lua = fixed_library("2.26");
    let log = frame(
        &lua,
        "nil",
        "",
        "ui:label('a', {x = 0, y = 0, w = 170, h = 40, text = 'Shrinking text', font = fonts.large, font_min = 16})\n\
         ui:label('b', {x = 0, y = 0, w = 170, h = 40, text = 'Shrinking text that never fits', font = fonts.large,\n\
           font_min = 16})\n\
         ui:label('c', {x = 0, y = 0, w = 170, h = 40, text = 'Shrinking text that never fits', font = fonts.large,\n\
           font_min = 21})\n\
         ui:button('d', {x = 0, y = 0, w = 120, h = 40, text = 'Shrink me', font = fonts.large, font_min = 12})\n\
         ui:button('e', {x = 0, y = 0, w = 140, h = 40, text = 'Shrink me', font = fonts.large, font_min = 12})",
    );
    let fonts: Vec<&str> = log
        .iter()
        .filter(|c| c.starts_with("text_box"))
        .map(|c| c.split("font=@").nth(1).unwrap().split(',').next().unwrap())
        .collect();
    // b fits only in small (two lines); c may not go below 21, so it keeps the smallest it tried
    assert_eq!(fonts, ["medium", "small", "medium", "medium", "large"]);
}

#[test]
fn button_image_and_skin_raise_at_the_app_line() {
    let lua = fixed_library("2.26");
    for field in ["image = {}", "skin = 'iron'"] {
        let code = format!(
            "local ui = make() ui:begin_frame(recorder({{}}))\n\
             ui:button('a', {{x = 0, y = 0, w = 10, h = 10, {field}}})"
        );
        let err = run(&lua, &code).unwrap_err();
        assert!(
            err.starts_with(
                "index.lua:2: ui:button image and skin are not implemented in inkemu yet"
            ),
            "{err}"
        );
    }
}

#[test]
fn a_canvas_without_pattern_gets_no_dots() {
    let lua = fixed_library("2.26");
    let decl = "ui:button('off', {x = 10, y = 0, w = 200, h = 56, text = 'B', enabled = false})";
    let dots = "pattern(10,0,200,56,{color=0,density=50,kind=dot})".to_string();
    assert_eq!(
        frame(&lua, "nil", "", decl),
        [body(0, false, 0), text(0, 15), dots].to_vec()
    );
    // the canvas of a device without the pattern capability, as the oracle's s15
    let code = format!(
        "local ui = make() local log = {{}} local g = recorder(log) g.pattern = nil\n\
         ui:begin_frame(g) {decl} ui:end_frame() return table.concat(log, ' ')"
    );
    let r = run(&lua, &code).unwrap();
    assert_eq!(
        r.into_iter().next().unwrap().to_string().unwrap(),
        format!("{} {}", body(0, false, 0), text(0, 15))
    );
}

#[test]
fn a_button_without_text_draws_only_its_box() {
    let lua = fixed_library("2.26");
    let decl = "ui:button('a', {x = 10, y = 0, w = 200, h = 56})\n\
         ui:button('b', {x = 10, y = 70, w = 200, h = 56, text = ''})\n\
         ui:button('c', {x = 10, y = 140, w = 200, h = 56, text = 42, font_min = 8})";
    let num = "text_box(10,140,200,56,42,{align=center,color=15,font=@medium,overflow=ellipsis,valign=middle,wrap=none})";
    assert_eq!(
        frame(&lua, "nil", "", decl),
        [
            body(0, false, 0),
            body(70, false, 0),
            body(140, false, 0),
            num.to_string()
        ]
        .to_vec()
    );
}

#[test]
fn a_label_hands_any_text_to_text_box() {
    let lua = fixed_library("2.26");
    let decl = "ui:label('n', {x = 0, y = 0, w = 10, h = 10, text = 42, font_min = 8})\n\
         ui:label('t', {x = 0, y = 0, w = 10, h = 10, text = true, font_min = 8})\n\
         ui:label('f', {x = 0, y = 0, w = 10, h = 10, text = false})\n\
         ui:label('z', {x = 0, y = 0, w = 10, h = 10})";
    let label =
        |text: &str| format!("text_box(0,0,10,10,{text},{{font=@medium,overflow=ellipsis}})");
    assert_eq!(
        frame(&lua, "nil", "", decl),
        [label("42"), label("true"), label("false"), label("")].to_vec()
    );
}

#[test]
fn a_device_without_a_touchscreen_draws_focus_from_the_start() {
    let lua = fixed_library("2.26");
    let decl = "ui:button('n', {x = 10, y = 0, w = 200, h = 56, text = 'B'})";
    // the library reads ctx.input.pointer when the object is created
    let code = format!(
        "input.pointer = false local ui = make() input.pointer = true local log = {{}}\n\
         for _ = 1, 2 do log = {{}} ui:begin_frame(recorder(log)) {decl} ui:end_frame() end\n\
         return table.concat(log, ' ')"
    );
    let r = run(&lua, &code).unwrap();
    assert_eq!(
        r.into_iter().next().unwrap().to_string().unwrap(),
        format!("{} {}", body(0, true, 0), text(0, 0))
    );
}

#[test]
fn a_label_text_the_canvas_refuses_is_an_app_error() {
    let core = Core::new(
        app_files(
            r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.33"}"#,
            "local ui = ctx.ui:create()\nfunction on_draw(ctx, g)\n  ui:begin_frame(g)\n\
             ui:label('a', {x = 0, y = 0, w = 10, h = 10, text = {}})\n  ui:end_frame()\nend",
        ),
        Config::default(),
        Persisted::default(),
    );
    let errors = core.errors().to_vec();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("bad argument #5 to 'text_box' (string expected, got table)")),
        "{errors:?}"
    );
}
