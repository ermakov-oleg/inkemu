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
        .lua_exec("local ui = ctx.ui:create()\nui:field('ok', {})")
        .unwrap_err();
    assert!(
        err.starts_with("lua_exec:2: ui:field is not implemented in inkemu yet"),
        "{err}"
    );
}

#[test]
fn a_library_error_in_a_callback_is_an_app_error() {
    let core = Core::new(
        app_files(
            r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.33"}"#,
            "local ui = ctx.ui:create()\nfunction on_draw(ctx, g)\n  ui:focus(5)\nend",
        ),
        Config::default(),
        Persisted::default(),
    );
    let errors = core.errors().to_vec();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("index.lua:3: ui:focus: id must be a string")),
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
    // a table with `_img = true` stands for an image object here
    let is_image = lua
        .load("return function(v) return type(v) == 'table' and v._img == true end")
        .eval::<Function>()
        .unwrap();
    host.set("is_image", is_image).unwrap();
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
  for _, op in ipairs({ "rect", "text_box", "pattern", "circle", "image" }) do
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
    // a table with `_img = true` stands for an image object here
    let is_image = lua
        .load("return function(v) return type(v) == 'table' and v._img == true end")
        .eval::<Function>()
        .unwrap();
    host.set("is_image", is_image).unwrap();
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

/// `bare`: a whole 20 × 20 image (no edges) with text colour 4; `dim` the same with a disabled image.
const BARE: &str = "{skins = {bare = {image = {_img = true, name = 'bare', width = 20, height = 20}, text_color = 4},\n\
     dim = {image = {_img = true, name = 'bare', width = 20, height = 20}, text_color = 4,\n\
       disabled = {image = {_img = true, name = 'off', width = 20, height = 20}}}}}";

#[test]
fn a_skinned_button_draws_its_look_then_the_text_the_ring_and_the_dots() {
    let lua = fixed_library("2.26");
    let look = |name: &str, invert: bool| {
        format!("image(@{name},10,0,{{h=40,invert={invert},src={{h=20,w=20,x=0,y=0}},w=100}})")
    };
    let text = "text_box(10,0,100,40,A,{align=center,color=4,font=@medium,overflow=ellipsis,valign=middle,wrap=none})";
    let ring = "rect(7,-3,106,46,{line_width=2,radius=0})";
    let dots = "pattern(10,0,100,40,{color=0,density=50,kind=dot})";
    let button = |more: &str| {
        format!("ui:button('a', {{x = 10, y = 0, w = 100, h = 40, text = 'A', radius = 9{more}}})")
    };
    assert_eq!(
        frame(&lua, BARE, "", &button(", skin = 'bare'")),
        [look("bare", false), text.into()]
    );
    // no selected look: the normal one inverted, the text colour too
    assert_eq!(
        frame(&lua, BARE, "", &button(", skin = 'bare', selected = true")),
        [look("bare", true), text.replace("color=4", "color=11")]
    );
    // the ring comes after the text, square whatever the radius
    assert_eq!(
        frame(
            &lua,
            BARE,
            "st.focus = 'a' st.keys = true",
            &button(", skin = 'bare'")
        ),
        [look("bare", false), text.into(), ring.into()]
    );
    assert_eq!(
        frame(&lua, BARE, "", &button(", skin = 'bare', enabled = false")),
        [look("bare", false), text.into(), dots.into()]
    );
    // a disabled look replaces the dots
    assert_eq!(
        frame(&lua, BARE, "", &button(", enabled = false, skin = 'dim'")),
        [look("off", false), text.into()]
    );
}

const PIC: &str = "local pic = {_img = true, name = 'pic', width = 48, height = 48}\n";

#[test]
fn an_image_button_draws_the_image_then_border_focus_frame_and_text() {
    let lua = fixed_library("2.26");
    let img = |y: i32, invert: bool| format!("image(@pic,10,{y},{{h=56,invert={invert},w=200}})");
    let decl = |id: &str, y: i32, more: &str| {
        format!("ui:button('{id}', {{x = 10, y = {y}, w = 200, h = 56, image = pic, {more}}})\n")
    };
    let body = [
        decl("n", 0, "border = false"),
        decl("sel", 70, "selected = true, text = 'B', border = false"),
        decl("emp", 140, "emphasis = true, border = false"),
        decl("r8", 210, "radius = 8"),
    ]
    .concat();
    let plain_text = "text_box(10,70,200,56,B,{align=center,font=@medium,overflow=ellipsis,valign=middle,wrap=none})";
    assert_eq!(
        frame(&lua, "nil", PIC, &body),
        [
            img(0, false),
            img(70, true),
            plain_text.to_string(),
            img(140, false),
            img(210, false),
            "rect(10,210,200,56,{radius=8})".to_string(),
        ]
    );
    // focus rings the box with no fill, whatever the style; the image keeps `selected` as its only inversion
    let log = frame(
        &lua,
        "nil",
        &format!("{PIC}st.keys = true"),
        &format!(
            "{}ui:focus('f')",
            decl("f", 0, "radius = 8, border = false")
        ),
    );
    assert_eq!(log[0], img(0, false));
    assert_eq!(log[1], "rect(7,-3,206,62,{line_width=2,radius=11})");
}

#[test]
fn an_image_control_draws_its_source_and_is_not_focusable() {
    let lua = fixed_library("2.26");
    let log = frame(
        &lua,
        "nil",
        PIC,
        "ui:image('i', {x = 10, y = 20, w = 100, h = 50, source = pic})\n\
         ui:image('o', {x = 0, y = 0, w = 1, h = 1})\n\
         assert(select('#', ui:image('z', {x = 0, y = 0, w = 0, h = 1, source = pic})) == 2)\n\
         assert(ui:focused() == nil)",
    );
    assert_eq!(log, ["image(@pic,10,20,{h=50,w=100})"]);
}

#[test]
fn only_an_image_object_is_an_image_for_a_button_and_a_layout_child() {
    let out = layout_run(
        "2.26",
        &format!(
            "{PIC}local function try(f) ui:begin_frame(g) local a, b = f() ui:end_frame() return tostring(a) .. ':' .. tostring(b) end\n\
             return try(function() return ui:button('a', {{x = 0, y = 0, w = 10, h = 10, image = 5}}) end),\n\
               try(function() return ui:button('a', {{x = 0, y = 0, w = 10, h = 10, image = {{width = 10, height = 10}}}}) end),\n\
               try(function() return ui:button('a', {{x = 0, y = 0, w = 10, h = 10, image = pic}}) end),\n\
               try(function() return ui:row('r', {{x = 0, y = 0, w = 10, h = 10, children = {{{{kind = 'image', source = {{}}}}}}}}) end)"
        ),
    );
    assert_eq!(
        out,
        "nil:invalid_argument,nil:invalid_argument,nil:nil,nil:invalid_argument"
    );
}

#[test]
fn a_skin_that_is_not_registered_is_invalid_on_every_control() {
    let out = layout_run(
        "2.26",
        "local function try(f) ui:begin_frame(g) local a, b = f() ui:end_frame() return tostring(a) .. ':' .. tostring(b) end\n\
         local function with(k, v) local o = {x = 0, y = 0, w = 10, h = 10, items = {'a'}} o[k] = v return o end\n\
         return try(function() return ui:button('a', with('skin', 'nope')) end),\n\
           try(function() return ui:button('a', with('skin', 5)) end),\n\
           try(function() return ui:button('a', {x = 0, y = 0, w = 10, h = 10, skin = 'iron', image = {}}) end),\n\
           try(function() return ui:radio('r', with('skin', 'nope')) end),\n\
           try(function() return ui:row('c', {x = 0, y = 0, w = 10, h = 10, children = {\n\
             {kind = 'button', id = 'b', grow = 1, skin = 'nope'}}}) end),\n\
           try(function() return ui:row('c', {x = 0, y = 0, w = 10, h = 10, children = {\n\
             {kind = 'button', id = 'b', grow = 1, skin = 'iron', image = {}}}}) end)",
    );
    assert_eq!(out, ["nil:invalid_argument"; 6].join(","));
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

/// Lua helpers for the event tests: `ui` (a new object), `frame(decl)` (one frame of `decl`, Lua using `ui`),
/// `ev(t)` (`handle_event(t)`'s returns joined with commas), `key(name[, phase])` and `point(phase, x, y, dx, dy
/// [, gesture])`.
const EVENTS: &str = "local ui = make() local g = recorder({})\n\
    local function show(...) local t = table.pack(...) for i = 1, t.n do t[i] = tostring(t[i]) end\n\
      return table.concat(t, ',', 1, t.n) end\n\
    local function frame(decl) ui:begin_frame(g) decl() ui:end_frame() end\n\
    local function ev(t) return show(ui:handle_event(t)) end\n\
    local function key(name, phase) return ev({type = 'key', key = name, phase = phase or 'down'}) end\n\
    local function point(phase, x, y, dx, dy, gesture)\n\
      return ev({type = 'pointer', phase = phase, x = x, y = y, down_x = dx, down_y = dy, gesture = gesture}) end\n";

fn events(api: &str, body: &str) -> String {
    let lua = fixed_library(api);
    let r = run(&lua, &format!("{EVENTS}{body}")).unwrap_or_else(|e| panic!("{e}"));
    r.into_iter().next().unwrap().to_string().unwrap()
}

#[test]
fn handle_event_refuses_what_is_not_a_key_or_pointer_event() {
    let out = events(
        "2.26",
        "return table.concat({ev(nil), ev('ok'), ev({}), ev({type = 'timer', phase = 'down'}),\n\
           ev({type = 'key', key = 'ok'}), key('ok', 'repeat'), key('down', 'repeat'), key('ok', 'bogus'),\n\
           ev({type = 'key', phase = 'down'}), key('back'), key('menu'), point('up'), point('down', 1, 1),\n\
           select('#', ui:handle_event({type = 'key', key = 'menu', phase = 'down'}))}, ' ')",
    );
    let bad = "nil,invalid_argument";
    assert_eq!(
        out,
        [
            bad, bad, bad, bad, bad, bad, bad, bad, "nil,nil", "nil,nil", "nil,nil", "nil,nil",
            "nil,nil", "2"
        ]
        .join(" ")
    );
}

#[test]
fn ok_up_activates_only_after_its_ok_down_on_the_same_focus() {
    let out = events(
        "2.26",
        "local out = {key('ok'), key('ok', 'up')}\n\
         frame(function()\n\
           ui:button('a', {x = 0, y = 0, w = 50, h = 50})\n\
           ui:button('b', {x = 60, y = 0, w = 50, h = 50})\n\
         end)\n\
         for _, k in ipairs({'ok', 'ok_up', 'ok_up', 'ok', 'right', 'ok_up', 'ok', 'left', 'right', 'ok_up',\n\
           'ok', 'right', 'ok_up', 'ok'}) do\n\
           out[#out + 1] = k == 'ok_up' and key('ok', 'up') or key(k)\n\
         end\n\
         frame(function()\n\
           ui:button('a', {x = 0, y = 0, w = 50, h = 50})\n\
           ui:button('b', {x = 60, y = 0, w = 50, h = 50})\n\
         end)\n\
         out[#out + 1] = key('ok', 'up')\n\
         ui:focus('b') out[#out + 1] = key('ok') ui:focus('a') ui:focus('b') out[#out + 1] = key('ok', 'up')\n\
         return table.concat(out, ' ')",
    );
    // a right arrow that finds nothing and a frame keep the pair; moving away and back, by arrows or ui:focus, breaks it
    assert_eq!(
        out,
        "nil,nil nil,nil a,press a,activate nil,nil a,press b,focus nil,nil b,press a,focus b,focus nil,nil \
         b,press nil,nil b,activate b,press b,activate b,press nil,nil"
    );
}

#[test]
fn a_tap_hits_the_last_added_target_at_the_press_point() {
    let out = events(
        "2.26",
        "frame(function()\n\
           ui:button('a', {x = 0, y = 0, w = 100, h = 100})\n\
           ui:region('b', {x = 50, y = 50, w = 100, h = 100})\n\
           ui:button('c', {x = 200, y = 0, w = 100, h = 100})\n\
           ui:button('d', {x = 200, y = 0, w = 50, h = 50, enabled = false})\n\
           ui:label('l', {x = 0, y = 0, w = 300, h = 300})\n\
         end)\n\
         local out = {}\n\
         local function add(r) out[#out + 1] = r .. '/' .. tostring(ui:focused()) end\n\
         add(point('up', 10, 10, nil, nil, 'tap'))\n\
         add(point('up', 60, 60, 60, 60, 'double_tap'))\n\
         add(point('up', 10, 10, 260, 60, 'tap'))\n\
         add(point('up', 260, 60, 10, 10, 'swipe_right'))\n\
         add(point('hold', 400, 400, 10, 10))\n\
         add(point('up', 210, 10, 210, 10, 'tap'))\n\
         add(point('up', 100, 10, 100, 10, 'tap'))\n\
         add(point('up', 99, 10, 99, 10, 'long'))\n\
         add(point('move', 10, 10, 10, 10))\n\
         return table.concat(out, ' ')",
    );
    // the label is not hittable; d is disabled and blocks c below it; x = 100 is past a's right edge
    assert_eq!(
        out,
        "a,activate/a b,activate/b c,activate/c nil,nil/c a,long_press/c nil,nil/c nil,nil/c nil,nil/c nil,nil/c"
    );
}

#[test]
fn focus_is_drawn_after_a_key_and_not_after_a_pointer_event() {
    let lua = fixed_library("2.26");
    let decl = "ui:button('n', {x = 10, y = 0, w = 200, h = 56, text = 'B'})";
    let code = format!(
        "local ui = make() local log\n\
         local function f() log = {{}} ui:begin_frame(recorder(log)) {decl} ui:end_frame() return log[1] end\n\
         local out = {{f()}}\n\
         ui:handle_event({{type = 'key', key = 'back', phase = 'up'}}) out[#out + 1] = f()\n\
         ui:handle_event({{type = 'pointer', phase = 'move', x = 0, y = 0}}) out[#out + 1] = f()\n\
         ui:handle_event({{type = 'key', key = 'ok', phase = 'repeat'}}) out[#out + 1] = f()\n\
         ui:handle_event({{type = 'key', key = 'zz', phase = 'down'}}) out[#out + 1] = f()\n\
         ui:handle_event({{type = 'pointer'}}) out[#out + 1] = f()\n\
         return table.concat(out, ' ')"
    );
    let r = run(&lua, &code).unwrap();
    let (off, on) = (body(0, false, 0), body(0, true, 0));
    assert_eq!(
        r.into_iter().next().unwrap().to_string().unwrap(),
        [&off, &on, &off, &off, &on, &off]
            .map(String::as_str)
            .join(" ")
    );
}

/// A 3×3 grid of 100×60 buttons g1..g9 140 px apart, and `nav` on g5.
const GRID: &str = "local function grid(nav)\n\
      frame(function()\n\
        for i = 1, 9 do\n\
          ui:button('g' .. i, {x = 40 + (i - 1) % 3 * 140, y = 300 + (i - 1) // 3 * 100, w = 100, h = 60,\n\
            nav = i == 5 and nav or nil})\n\
        end\n\
      end)\n\
    end\n\
    local function walk(from, keys)\n\
      local out = {}\n\
      for k in string.gmatch(keys, '%a+') do ui:focus(from) out[#out + 1] = key(k) end\n\
      return table.concat(out, ' ')\n\
    end\n";

#[test]
fn arrows_move_by_position_from_2_13_and_in_declaration_order_before() {
    let body =
        "grid() local out = {walk('g5', 'up down left right'), walk('g1', 'up left down right'),\n\
                walk('g9', 'down right up left')}\n\
                grid({down = 'g1', up = 'nobody'}) out[#out + 1] = walk('g5', 'down up')\n\
                return table.concat(out, ' / ')";
    assert_eq!(
        events("2.26", &format!("{GRID}{body}")),
        "g2,focus g8,focus g4,focus g6,focus / nil,nil nil,nil g4,focus g2,focus / \
         nil,nil nil,nil g6,focus g8,focus / g1,focus g2,focus"
    );
    assert_eq!(
        events("2.12", &format!("{GRID}{body}")),
        "g4,focus g6,focus g4,focus g6,focus / g1,focus g1,focus g2,focus g2,focus / \
         g9,focus g9,focus g8,focus g8,focus / g6,focus g4,focus"
    );
}

#[test]
fn an_arrow_without_a_known_focus_falls_on_the_first_available_target() {
    let out = events(
        "2.26",
        "local out = {key('down')}\n\
         frame(function()\n\
           ui:button('a', {x = 0, y = 0, w = 50, h = 50, enabled = false})\n\
           ui:button('b', {x = 0, y = 60, w = 50, h = 50})\n\
         end)\n\
         ui:focus('ghost') out[#out + 1] = key('up')\n\
         frame(function() ui:label('l', {x = 0, y = 0, w = 50, h = 50}) end)\n\
         out[#out + 1] = tostring(ui:focused()) out[#out + 1] = key('down')\n\
         return table.concat(out, ' ')",
    );
    assert_eq!(out, "nil,nil b,focus nil nil,nil");
}

#[test]
fn focus_keeps_a_part_until_end_frame() {
    let out = events(
        "2.26",
        "local out = {}\n\
         local function add(...) out[#out + 1] = show(...) end\n\
         add(ui:focus('b', 2.0)) add(ui:focused()) add(key('ok'))\n\
         add(ui:focus('b', 0)) add(ui:focus('b', 1.5)) add(ui:focus('b', '2')) add(ui:focus(''))\n\
         add(pcall(ui.focus, ui, 5)) add(math.type(select(2, ui:focused())))\n\
         frame(function() ui:button('b', {x = 0, y = 0, w = 50, h = 50}) end)\n\
         add(ui:focused())\n\
         return table.concat(out, ' ')",
    );
    assert_eq!(
        out,
        " b,2 b,press,2 nil,invalid_argument nil,invalid_argument nil,invalid_argument nil,invalid_argument \
         false,ui:focus: id must be a string integer b"
    );
}

/// Skins for `make`: `iron`, a 40 × 30 image with 10 px edges, text padded 6 and 12, text colour 3 and a selected
/// image of 40 × 32.
const IRON: &str = "{iron = {image = {_img = true, name = 'iron', width = 40, height = 30}, edge = 10, pad = {6, 12}, \
     text_color = 3, selected = {image = {_img = true, name = 'iron_on', width = 40, height = 32}}}}";

/// What `code` (Lua using `ui`, an object made with the [`IRON`] skins, and `g`, a recorder) returns as text, or its
/// error.
fn layout_run(api: &str, code: &str) -> String {
    let lua = fixed_library(api);
    let code = format!(
        "local ui = make({{skins = {IRON}}}) local log = {{}} local g = recorder(log)\n{code}"
    );
    match run(&lua, &code) {
        Ok(r) => r
            .into_iter()
            .map(|v| v.to_string().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(","),
        Err(e) => e,
    }
}

#[test]
fn containers_raise_what_inkemu_does_not_implement_at_the_app_line() {
    for kind in ["column", "grid", "stack"] {
        let code = format!(
            "ui:begin_frame(g)\nui:{kind}('c', {{x = 0, y = 0, w = 100, h = 100, columns = {cols}, children = {{{{kind = 'field', id = 's', w = 10, h = 10}}}}}})",
            cols = if kind == "grid" { "1" } else { "nil" }
        );
        let err = layout_run("2.26", &code);
        assert!(
            err.starts_with("index.lua:3: ui:field is not implemented in inkemu yet"),
            "{code}: {err}"
        );
    }
    // an invalid field anywhere in the tree wins over a part inkemu lacks; ui:layout takes a registered container
    // skin and draws nothing
    assert_eq!(
        layout_run(
            "2.26",
            "ui:begin_frame(g)\nreturn ui:column('c', {x = 0, y = 0, w = 100, h = 100, children = {\n\
               {kind = 'switch', id = 's'}, {kind = 'button', id = 'b', grow = -1}}})"
        ),
        "nil,invalid_argument"
    );
    assert_eq!(
        layout_run(
            "2.26",
            "return type(ui:layout({x = 0, y = 0, w = 9, h = 9}, {kind = 'row', skin = 'iron', children = {}}))"
        ),
        "table"
    );
}

#[test]
fn ui_layout_counts_toward_no_frame() {
    let out = layout_run(
        "2.26",
        "ui:begin_frame(g)\n\
         for i = 1, 128 do ui:label('l' .. i, {x = 0, y = 0, w = 1, h = 1}) end\n\
         local b = ui:layout({x = 0, y = 0, w = 100, h = 10}, {kind = 'row', children = {{id = 'a', grow = 1}}})\n\
         local r = table.pack(ui:label('x', {x = 0, y = 0, w = 1, h = 1}))\n\
         ui:end_frame()\n\
         local c = ui:layout({x = 0, y = 0, w = 100, h = 10}, {kind = 'row', children = {{id = 'a', grow = 1}}})\n\
         return b.a.w, r[2], c.a.w",
    );
    assert_eq!(out, "100,limit_exceeded,100");
}

#[test]
fn a_container_registers_itself_before_its_children_in_declaration_order() {
    let lua = fixed_library("2.26");
    let log = frame(
        &lua,
        "nil",
        "st.keys = true",
        "ui:column('page', {x = 0, y = 0, w = 200, h = 200, background = 15, children = {\n\
           {kind = 'row', h = 50, border = true, children = {\n\
             {kind = 'button', id = 'a', text = 'A', grow = 1}, {kind = 'button', id = 'b', text = 'B', grow = 1}}},\n\
           {kind = 'button', id = 'c', text = 'C', h = 50}}})",
    );
    let ops: Vec<&str> = log.iter().map(|c| c.split(',').next().unwrap()).collect();
    // the page's background, the row's border, then a (focused, so filled), b and c
    assert_eq!(
        ops.join(" "),
        "rect(0 rect(0 rect(0 text_box(0 rect(100 text_box(100 rect(0 text_box(0"
    );
    assert!(log[0].contains("color=15,fill=true,radius=0"), "{log:?}");
    assert!(log[1].contains("line_width=1"), "{log:?}");
    assert!(log[2].starts_with("rect(0,0,100,50,{fill=true"), "{log:?}");
}

#[test]
fn a_container_on_a_canvas_without_pattern_fills_with_its_colour() {
    let code = "g.pattern = %s ui:begin_frame(g)\n\
         ui:column('c', {x = 0, y = 0, w = 100, h = 50, background = 5, radius = 4, children = {}})\n\
         ui:end_frame() return table.concat(log, ' ')";
    let plain = layout_run("2.26", &code.replace("%s", "nil"));
    assert_eq!(plain, "rect(0,0,100,50,{color=5,fill=true,radius=4})");
    let dots = layout_run(
        "2.26",
        &code.replace("%s", "function(self, ...) log[#log + 1] = 'pattern' end"),
    );
    // paper, two dot rows per corner row (radius 4), the middle
    assert_eq!(
        dots,
        format!(
            "rect(0,0,100,50,{{color=0,fill=true,radius=4}}){}",
            " pattern".repeat(9)
        )
    );
}

#[test]
fn before_2_15_children_follow_each_other_and_nest() {
    let lua = fixed_library("2.12");
    let log = frame(
        &lua,
        "nil",
        "",
        "ui:column('col', {x = 10, y = 20, w = 100, h = 100, gap = 5, children = {\n\
           {kind = 'row', w = 90, h = 30, gap = 3, children = {\n\
             {kind = 'button', id = 'a', w = 20, h = 30}, {kind = 'button', id = 'b', w = 20, h = 30}}},\n\
           {kind = 'button', id = 'c', w = 40, h = 10}}})",
    );
    assert_eq!(
        log,
        [
            "rect(10,20,20,30,{fill=false,radius=0})",
            "rect(33,20,20,30,{fill=false,radius=0})",
            "rect(10,55,40,10,{fill=false,radius=0})",
        ]
    );
}

fn radio_text(x: i32, y: i32, w: i32, h: i32, item: &str) -> String {
    format!(
        "text_box({x},{y},{w},{h},{item},{{align=left,color=15,font=@medium,overflow=ellipsis,valign=middle,wrap=none}})"
    )
}

#[test]
fn radio_splits_its_box_into_cells_and_draws_each_with_its_focus_frame() {
    let lua = fixed_library("2.26");
    let log = frame(
        &lua,
        "nil",
        "st.keys = true st.focus = 'r' st.part = 3",
        "ui:radio('r', {x = 10, y = 20, w = 200, h = 100, items = {'a', 'b', 'c', 'd', 'e'}, value = 2,\n\
           columns = 2, gap = 4})",
    );
    // columns start at 0 and 4 + 196 // 2; rows at 0, 4 + 92 // 3 and 8 + 2 * 92 // 3; the last ones end at the box
    assert_eq!(
        log,
        [
            "circle(25,35,15,{line_width=2})".to_string(),
            radio_text(48, 20, 60, 30, "a"),
            "circle(127,35,15,{line_width=2})".to_string(),
            "circle(127,35,10,{fill=true})".to_string(),
            radio_text(150, 20, 60, 30, "b"),
            "circle(25,69,15,{line_width=2})".to_string(),
            radio_text(49, 54, 59, 31, "c"),
            "rect(7,51,104,37,{line_width=2,radius=0})".to_string(),
            "circle(127,69,15,{line_width=2})".to_string(),
            radio_text(151, 54, 59, 31, "d"),
            "circle(25,104,15,{line_width=2})".to_string(),
            radio_text(49, 89, 59, 31, "e"),
        ]
    );
}

#[test]
fn a_radio_cell_too_small_for_the_dot_has_none() {
    let lua = fixed_library("2.26");
    let log = frame(
        &lua,
        "nil",
        "",
        "ui:radio('r', {x = 0, y = 0, w = 100, h = 21, items = {'a', 'b'}, value = 2})",
    );
    // cells of 10 and 11: a dot needs a diameter s - 10 of at least 1
    assert_eq!(
        log.iter().filter(|c| c.contains("fill=true")).count(),
        1,
        "{log:?}"
    );
    let log = frame(
        &lua,
        "nil",
        "",
        "ui:radio('r', {x = 0, y = 0, w = 100, h = 21, items = {'a', 'b'}, value = 1})",
    );
    assert!(!log.iter().any(|c| c.contains("fill=true")), "{log:?}");
}

#[test]
fn radio_parts_take_hits_focus_and_return_their_number() {
    let out = events(
        "2.26",
        "local function scene(o)\n\
           o = o or {}\n\
           frame(function()\n\
             ui:button('top', {x = 0, y = 0, w = 100, h = 40, nav = o.nav})\n\
             ui:radio('r', {x = 0, y = 50, w = 200, h = 90, items = {'a', 'b', 'c'}, value = 2, enabled = o.enabled})\n\
             ui:button('bot', {x = 0, y = 150, w = 100, h = 40})\n\
           end)\n\
         end\n\
         local out = {}\n\
         local function add(r) out[#out + 1] = r end\n\
         scene()\n\
         add(point('up', 10, 60, 10, 60, 'tap')) add(show(ui:focused()))\n\
         add(point('up', 10, 90, 10, 90, 'tap')) add(point('hold', 10, 120, 10, 120)) add(show(ui:focused()))\n\
         add(key('down')) add(key('ok')) add(key('ok', 'up')) add(key('down')) add(key('up'))\n\
         ui:focus('r') add(show(ui:focused())) scene() add(show(ui:focused()))\n\
         ui:focus('r', 7) scene() add(show(ui:focused()))\n\
         ui:focus('top') scene() add(key('down'))\n\
         scene({nav = {down = 'r'}}) ui:focus('top') add(key('down'))\n\
         ui:focus('r', 3) scene({enabled = false}) add(show(ui:focused()))\n\
         add(point('up', 10, 60, 10, 60, 'tap'))\n\
         return table.concat(out, ' ')",
    );
    // a part that is not the value changes it, the value part activates; a composite without a part, or with one it
    // lacks, settles on its value part; the first part is the nearest one below `top`; a nav entry naming the radio
    // goes to its value part; a disabled radio hands focus to the nearest control and takes no taps
    assert_eq!(
        out,
        "r,change,1 r,1 r,activate,2 r,long_press,3 r,2 r,focus,3 r,press,3 r,change,3 bot,focus r,focus,3 \
         r r,2 r,2 r,focus,1 r,focus,2 bot nil,nil"
    );
}

#[test]
fn a_radio_counts_itself_and_each_item_against_the_limit() {
    let out = layout_run(
        "2.26",
        "local function try(n)\n\
           ui:begin_frame(g)\n\
           for i = 1, 120 do ui:label('l' .. i, {x = 0, y = 0, w = 1, h = 1}) end\n\
           local items = {} for i = 1, n do items[i] = 'i' end\n\
           local r = table.pack(ui:radio('r', {x = 0, y = 0, w = 100, h = 100, items = items}))\n\
           ui:end_frame()\n\
           return r.n .. ':' .. tostring(r[2])\n\
         end\n\
         return try(7), try(8)",
    );
    assert_eq!(out, "0:nil,2:limit_exceeded");
}

#[test]
fn radio_checks_its_fields() {
    let code = "ui:begin_frame(g)\n\
         local function try(o)\n\
           local t = {x = 0, y = 0, w = 100, h = 100, items = {'a', 'b'}}\n\
           for k, v in pairs(o) do t[k] = v end\n\
           local r = table.pack(ui:radio('r' .. tostring(next(o)) .. tostring(select(2, next(o))), t))\n\
           return r.n == 0 and 'ok' or tostring(r[2])\n\
         end\n\
         local out = {}\n\
         for _, o in ipairs({{value = 0}, {value = 3}, {value = 1.5}, {value = 2.0}, {columns = 3}, {columns = 2},\n\
           {gap = -1}, {gap = 300}, {align = 'justify'}, {align = 'center'}, {items = {}}, {items = {'a', 5}},\n\
           {enabled = 1}, {font_min = 7}, {nav = 5}, {nav = {up = 5}}, {nav = {up = 'x'}}, {bold = 1},
           {skin = 'nope'}}) do\n\
           out[#out + 1] = try(o)\n\
         end\n\
         return table.concat(out, ' ')";
    let bad = "invalid_argument";
    let ok = "ok";
    assert_eq!(
        layout_run("2.26", code),
        [
            bad, bad, bad, ok, bad, ok, bad, ok, bad, ok, bad, bad, bad, bad, bad, bad, ok, bad,
            bad
        ]
        .join(" ")
    );
    // `bold` is a 2.16 field
    assert!(layout_run("2.15", code).ends_with(&format!("{ok} {bad}")));
    assert_eq!(
        layout_run(
            "2.26",
            "ui:begin_frame(g)\nreturn select('#', ui:radio('r', {x = 0, y = 0, w = 9, h = 9, items = {'a'}, skin = 'iron'}))",
        ),
        "0"
    );
}

/// The nine `image` calls of a skin `name` (`iw` × `ih`, the same edge `e` all round) on a box at least `2e` both ways.
fn slices(
    name: &str,
    (iw, ih): (i32, i32),
    e: i32,
    (x, y, w, h): (i32, i32, i32, i32),
) -> Vec<String> {
    let cols = [
        (x, e, 0, e),
        (x + e, w - 2 * e, e, iw - 2 * e),
        (x + w - e, e, iw - e, e),
    ];
    let rows = [
        (y, e, 0, e),
        (y + e, h - 2 * e, e, ih - 2 * e),
        (y + h - e, e, ih - e, e),
    ];
    rows.iter()
        .flat_map(|&(dy, dh, sy, sh)| {
            cols.iter().map(move |&(dx, dw, sx, sw)| {
                format!("image(@{name},{dx},{dy},{{h={dh},invert=false,src={{h={sh},w={sw},x={sx},y={sy}}},w={dw}}})")
            })
        })
        .collect()
}

#[test]
fn a_skinned_radio_and_switch_draw_the_skin_in_place_of_circles_and_capsule() {
    let lua = fixed_library("2.26");
    let opts = format!("{{skins = {IRON}}}");
    // each cell gets the skin, selected for the value item, and its text in the area after the pad
    let cell = |x: i32, text: &str| {
        format!("text_box({},86,206,88,{text},{{align=left,color=3,font=@medium,overflow=ellipsis,valign=middle,wrap=none}})", x + 12)
    };
    let want = [
        slices("iron_on", (40, 32), 10, (10, 80, 230, 100)),
        vec![cell(10, "x")],
        slices("iron", (40, 30), 10, (240, 80, 230, 100)),
        vec![cell(240, "y")],
    ]
    .concat();
    let decl = "ui:radio('rd', {x = 10, y = 80, w = 460, h = 100, columns = 2, items = {'x', 'y'}, value = 1, skin = 'iron'})";
    assert_eq!(frame(&lua, &opts, "", decl), want);
    // the skin takes the capsule's box (64 × 34, 8 px from the right, centred), the text keeps its place
    let want = [
        slices("iron", (40, 30), 10, (338, 711, 64, 34)),
        vec!["text_box(18,700,308,56,Sw,{bold=true,font=@medium,overflow=ellipsis,valign=middle,wrap=none})".into()],
    ]
    .concat();
    let decl = "ui:switch('sw', {x = 10, y = 700, w = 400, h = 56, text = 'Sw', skin = 'iron'})";
    assert_eq!(frame(&lua, &opts, "", decl), want);
    let on = frame(
        &lua,
        &opts,
        "",
        &decl.replace("skin", "selected = true, skin"),
    );
    assert_eq!(on[..9], slices("iron_on", (40, 32), 10, (338, 711, 64, 34)));
}

#[test]
fn a_radio_child_splits_the_box_its_container_gives_it() {
    let lua = fixed_library("2.26");
    let log = frame(
        &lua,
        "nil",
        "",
        "ui:column('c', {x = 0, y = 0, w = 200, h = 100, padding = 10, children = {\n\
           {kind = 'radio', id = 'r', items = {'a', 'b'}, h = 60}}})",
    );
    assert_eq!(
        log,
        [
            "circle(25,25,15,{line_width=2})".to_string(),
            radio_text(48, 10, 142, 30, "a"),
            "circle(25,55,15,{line_width=2})".to_string(),
            radio_text(48, 40, 142, 30, "b"),
        ]
    );
}

#[test]
fn before_2_13_the_arrows_step_through_radio_parts_in_order() {
    let out = events(
        "2.12",
        "frame(function()\n\
           ui:button('a', {x = 0, y = 0, w = 100, h = 40})\n\
           ui:radio('r', {x = 0, y = 50, w = 200, h = 90, items = {'x', 'y', 'z'}, value = 2})\n\
           ui:button('b', {x = 0, y = 150, w = 100, h = 40})\n\
         end)\n\
         local out = {}\n\
         for _, k in ipairs({'down', 'down', 'down', 'down', 'down', 'down', 'up', 'up', 'up', 'up', 'up'}) do\n\
           out[#out + 1] = key(k)\n\
         end\n\
         return table.concat(out, ' ')",
    );
    // focus starts on a; each item is a stop of its own, and at either end the same target is returned
    assert_eq!(
        out,
        "r,focus,1 r,focus,2 r,focus,3 b,focus b,focus b,focus r,focus,3 r,focus,2 r,focus,1 a,focus a,focus"
    );
}

#[test]
fn ok_acts_on_the_part_focus_stands_for() {
    let out = events(
        "2.26",
        "local function scene(enabled)\n\
           frame(function()\n\
             ui:button('top', {x = 0, y = 0, w = 100, h = 40})\n\
             ui:radio('r', {x = 0, y = 50, w = 200, h = 90, items = {'a', 'b', 'c'}, value = 2, enabled = enabled})\n\
           end)\n\
         end\n\
         local out = {}\n\
         scene()\n\
         ui:focus('r', 7) out[#out + 1] = key('ok') out[#out + 1] = key('ok', 'up')\n\
         ui:focus('r') out[#out + 1] = key('ok') out[#out + 1] = key('ok', 'up')\n\
         scene(false)\n\
         ui:focus('r') out[#out + 1] = key('ok')\n\
         ui:focus('r', 1) out[#out + 1] = key('ok')\n\
         return table.concat(out, ' ')",
    );
    // a part the radio lacks, or none, stands for the value part; a disabled radio gives nothing
    assert_eq!(
        out,
        "r,press,2 r,activate,2 r,press,2 r,activate,2 nil,nil nil,nil"
    );
}

#[test]
fn narrow_radio_cells_never_get_a_negative_size() {
    let lua = fixed_library("2.26");
    let log = frame(
        &lua,
        "nil",
        "",
        "ui:radio('n', {x = 0, y = 0, w = 30, h = 40, items = {'a'}})\n\
         ui:radio('g', {x = 0, y = 100, w = 100, h = 20, items = {'a', 'b', 'c'}, gap = 2000000000})",
    );
    assert_eq!(log[1], radio_text(40, 0, 0, 40, "a"));
    // the gap counts as the whole height: every cell is 0 tall but the last, which ends at the box
    assert_eq!(log[2], "circle(0,100,0,{line_width=2})");
    assert!(!log.iter().any(|c| c.contains(",-")), "{log:?}");
}

#[test]
fn a_frame_checks_each_child_as_it_registers_it() {
    let out = layout_run(
        "2.26",
        "local function try(children)\n\
           ui:begin_frame(g)\n\
           local r = table.pack(ui:column('c', {x = 0, y = 0, w = 100, h = 100, children = children}))\n\
           ui:end_frame()\n\
           return (r.n == 0 and 'ok' or tostring(r[2])) .. ':' .. tostring(ui:focused())\n\
         end\n\
         return try({{kind = 'button', id = 'a', h = 20}, {kind = 'button', id = 'b', h = 20, enabled = 'no'}}),\n\
           try({{kind = 'button', id = 'a', h = 20}, {kind = 'button', id = 'h', h = 20, hidden = true, enabled = 1}}),\n\
           try({{kind = 'button', id = 'a', h = 20}, {kind = 'button', id = 'z', h = 20, min_h = 0, max_h = 0,\n\
             enabled = 1}}),\n\
           try({{kind = 'button', id = 'a', h = 20}, {kind = 'button', id = 'b', h = 20, grow = -1}}),\n\
           try({{kind = 'button', id = 'a', h = 20, text = 7}, {kind = 'label', id = 'l', h = 20, text = true}})",
    );
    // a bad own field keeps the child before it, even on a hidden or zero-height child; a bad sizing field refuses
    // the whole call; texts that are not strings are taken where nothing measures them
    assert_eq!(
        out,
        "invalid_argument:a,invalid_argument:a,invalid_argument:a,invalid_argument:nil,ok:a"
    );
}

#[test]
fn a_row_child_with_only_max_h_keeps_its_width() {
    let out = layout_run(
        "2.26",
        "local R = {x = 0, y = 0, w = 300, h = 100}\n\
         local function w(child) return ui:layout(R, {kind = 'row', children = {child}})[1].w end\n\
         local function h(child)\n\
           return ui:layout(R, {kind = 'column', align_items = 'start', children = {child}})[1].h\n\
         end\n\
         return w({grow = 1, max_h = 20}), w({w = 200, max_h = 20}),\n\
           w({kind = 'button', text = 'ok', w = 'fit', max_h = 30}), h({grow = 1, max_w = 20}),\n\
           h({h = 50, max_w = 20})",
    );
    // a fitted button is its text (2 x 12 px) plus 24
    assert_eq!(out, "300,200,48,100,50");
}

/// Lua that sets `add(name, f)` and `check()`: `f(v)` declares something with one integer field set to `v`;
/// `check()` tries every case with whole floats past 32 bits and lists the ones not refused with invalid_argument.
const SWEEP: &str = r#"
local cases = {}
local function add(name, f) cases[#cases + 1] = { name, f } end
local function with(t, k, v) local c = {} for key, val in pairs(t) do c[key] = val end c[k] = v return c end
local function framed(kind, id, o)
  local ui = make() ui:begin_frame(g) local a, b = ui[kind](ui, id, o) ui:end_frame() return a, b
end
local function layout(rect, spec) return make():layout(rect, spec) end
local function check()
  local bad = {}
  for _, v in ipairs({ 3e9, -3e9, 1e10 }) do
    for _, c in ipairs(cases) do
      local r = table.pack(pcall(c[2], v))
      if not (r[1] and r.n == 3 and r[2] == nil and r[3] == "invalid_argument") then
        bad[#bad + 1] = c[1] .. "=" .. tostring(v) .. ": " .. tostring(r[2]) .. "," .. tostring(r[3])
      end
    end
  end
  return table.concat(bad, "\n")
end
"#;

#[test]
fn whole_floats_past_32_bits_are_invalid_arguments() {
    let new_rules = "local R = {x = 0, y = 0, w = 100, h = 100}\n\
        local ROW = {x = 0, y = 0, w = 100, h = 100, children = {{kind = 'button', id = 'b', grow = 1}}}\n\
        for _, k in ipairs({'x', 'y', 'w', 'h'}) do\n\
          add('rect.' .. k, function(v) return layout(with(R, k, v), {kind = 'row', children = {{grow = 1}}}) end)\n\
          add('row.' .. k, function(v) return framed('row', 'r', with(ROW, k, v)) end)\n\
          add('button.' .. k, function(v) return framed('button', 'b', with(R, k, v)) end)\n\
          add('radio.' .. k, function(v) return framed('radio', 'r', with(with(R, 'items', {'a'}), k, v)) end)\n\
        end\n\
        for _, k in ipairs({'w', 'h', 'min_w', 'max_w', 'min_h', 'max_h', 'grow', 'shrink'}) do\n\
          add('child.' .. k, function(v) return layout(R, {kind = 'row', children = {with({grow = 1}, k, v)}}) end)\n\
          add('frame_child.' .. k, function(v)\n\
            return framed('row', 'r', with(ROW, 'children', {with({kind = 'button', id = 'b', grow = 1}, k, v)}))\n\
          end)\n\
        end\n\
        for _, k in ipairs({'padding', 'gap', 'row_gap', 'column_gap', 'line_width', 'border_color', 'background',\n\
            'radius'}) do\n\
          add('row.' .. k, function(v) return layout(R, with({kind = 'row', children = {{grow = 1}}}, k, v)) end)\n\
        end\n\
        add('row.padding_list', function(v) return layout(R, {kind = 'row', padding = {v, 1}, children = {}}) end)\n\
        for _, k in ipairs({'line_width', 'color', 'inset'}) do\n\
          add('divider.' .. k, function(v)\n\
            return layout(R, {kind = 'column', divider = {[k] = v}, children = {{grow = 1}, {grow = 1}}})\n\
          end)\n\
        end\n\
        for _, k in ipairs({'gap', 'value', 'columns'}) do\n\
          add('radio.' .. k, function(v) return framed('radio', 'r', with(with(R, 'items', {'a', 'b'}), k, v)) end)\n\
        end\n\
        add('button.font_min', function(v) return framed('button', 'b', with(R, 'font_min', v)) end)\n\
        add('grid.columns', function(v) return layout(R, {kind = 'grid', columns = v}) end)\n\
        add('grid.min', function(v) return layout(R, {kind = 'grid', columns = {min = v}}) end)\n\
        add('grid.track', function(v) return layout(R, {kind = 'grid', columns = {v}}) end)\n\
        add('grid.rows', function(v) return layout(R, {kind = 'grid', columns = 1, rows = {v}}) end)\n\
        add('grid.auto_rows', function(v) return layout(R, {kind = 'grid', columns = 1, auto_rows = v}) end)\n\
        for _, k in ipairs({'column_span', 'row_span'}) do\n\
          add('grid.' .. k, function(v) return layout(R, {kind = 'grid', columns = 2, children = {{[k] = v}}}) end)\n\
        end\n\
        add('focus.part', function(v) return make():focus('a', v) end)\n\
        return check()";
    assert_eq!(layout_run("2.26", &format!("{SWEEP}{new_rules}")), "");
    let old_rules = "local ROW = {x = 0, y = 0, w = 100, h = 100, children = {{kind = 'button', id = 'b', w = 10, h = 10}}}\n\
        for _, k in ipairs({'x', 'y', 'w', 'h', 'gap'}) do\n\
          add('row.' .. k, function(v) return framed('row', 'r', with(ROW, k, v)) end)\n\
        end\n\
        for _, k in ipairs({'w', 'h'}) do\n\
          add('child.' .. k, function(v)\n\
            return framed('row', 'r', with(ROW, 'children', {with({kind = 'button', id = 'b', w = 10, h = 10}, k, v)}))\n\
          end)\n\
        end\n\
        return check()";
    assert_eq!(layout_run("2.14", &format!("{SWEEP}{old_rules}")), "");
}

#[test]
fn grid_choices_the_oracle_does_not_pin() {
    let out = layout_run(
        "2.26",
        "local function boxes(b)\n\
           local out = {}\n\
           for i = 1, #b do out[i] = b[i].x .. ',' .. b[i].y .. ',' .. b[i].w .. ',' .. b[i].h end\n\
           return table.concat(out, ' ')\n\
         end\n\
         local R = {x = 0, y = 0, w = 300, h = 'fit'}\n\
         local spans = ui:layout(R, {kind = 'grid', columns = 3, auto_rows = 10, children = {\n\
           {column_span = 2}, {row_span = 2}, {}, {column_span = 3, row_span = 2}, {}}})\n\
         local cut = ui:layout({x = 0, y = 0, w = 480, h = 10}, {kind = 'grid', columns = {min = 200},\n\
           children = {{column_span = 3}}})\n\
         local rows = ui:layout(R, {kind = 'grid', columns = 1, rows = {50, 50}, children = {{}}})\n\
         return boxes(spans), boxes(cut), rows.h",
    );
    // a row span keeps its rows taken for later children; a span longer than the `{ min }` columns is cut to them;
    // explicit rows count even when no child reaches them
    assert_eq!(
        out,
        "0,0,200,10 200,0,100,20 0,10,100,10 0,20,300,20 0,40,100,10,0,0,480,1,100"
    );
}
