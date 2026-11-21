use std::collections::BTreeMap;

use inkcore::{AppFiles, Config, Core, Persisted};
use serde_json::json;

/// A 2×2 grayscale PNG: black, white / white, black.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x00, 0x00, 0x00, 0x00, 0x57, 0xdd, 0x52,
    0xf8, 0x00, 0x00, 0x00, 0x0e, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60, 0xf8, 0xcf, 0xf0,
    0x9f, 0x01, 0x00, 0x06, 0x00, 0x01, 0xff, 0x8f, 0xf1, 0xfc, 0x9a, 0x00, 0x00, 0x00, 0x00, 0x49,
    0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn app_with(index_lua: &str, extra: &[(&str, &[u8])], permissions: &str) -> AppFiles {
    let manifest = format!(
        r#"{{"app_id": "testapp", "entry": "index.lua", "api": "2.2", "permissions": [{permissions}]}}"#
    );
    let mut files = BTreeMap::from([
        ("manifest.json".to_string(), manifest.into_bytes()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]);
    for (k, v) in extra {
        files.insert(k.to_string(), v.to_vec());
    }
    AppFiles(files)
}

fn boot(index_lua: &str) -> Core {
    Core::new(
        app_with(index_lua, &[], ""),
        Config::default(),
        Persisted::default(),
    )
}

fn no_errors(core: &Core) {
    assert!(core.errors().is_empty(), "{:?}", core.errors().to_vec());
}

#[test]
fn storage_round_trips_binary_bytes() {
    let mut core = boot("function on_load(c) assert(c.storage:write('b', '\\0\\1\\255')) end");
    no_errors(&core);
    assert_eq!(
        core.lua_eval("local s = ctx.storage:read('b') return {#s, s:byte(1, -1)}")
            .unwrap(),
        json!([3, 0, 1, 255])
    );
    assert_eq!(
        core.lua_eval("return ctx.storage:read('b') == '\\0\\1\\255'")
            .unwrap(),
        json!(true)
    );
    let p = core.take_persist().expect("files changed");
    assert_eq!(
        p.files.get("b").map(Vec::as_slice),
        Some(&[0u8, 1, 255][..])
    );
}

#[test]
fn the_513th_file_has_no_space() {
    let mut core = boot(
        "function on_load(c)\n\
           for i = 1, 512 do assert(c.storage:write('f' .. i, 'x')) end\n\
           r, e = c.storage:write('f513', 'x')\n\
           again = c.storage:write('f1', 'yy')\n\
         end",
    );
    no_errors(&core);
    assert_eq!(
        core.lua_eval("return {tostring(r), e, again, ctx.storage:usage().used_files}")
            .unwrap(),
        json!(["nil", "no_space", true, 512])
    );
}

#[test]
fn files_seeded_on_disk_are_readable_and_removable() {
    let persisted = Persisted {
        files: BTreeMap::from([("cal.txt".to_string(), "Событие".as_bytes().to_vec())]),
        ..Persisted::default()
    };
    let mut core = Core::new(app_with("", &[], ""), Config::default(), persisted);
    assert_eq!(
        core.lua_eval("return ctx.files:read('cal.txt')").unwrap(),
        json!("Событие")
    );
    assert_eq!(
        core.lua_eval("return ctx.files:remove('cal.txt')").unwrap(),
        json!(true)
    );
    assert!(core.take_persist().unwrap().files.is_empty());
}

#[test]
fn package_read_of_a_png_returns_its_exact_bytes() {
    let mut core = Core::new(
        app_with("", &[("data/img.png", PNG)], ""),
        Config::default(),
        Persisted::default(),
    );
    let bytes = core
        .lua_eval("local s = ctx.package:read('img.png') return {s:byte(1, -1)}")
        .unwrap();
    let expected: Vec<serde_json::Value> = PNG.iter().map(|b| json!(b)).collect();
    assert_eq!(bytes, json!(expected));
    assert_eq!(
        core.lua_eval("return ctx.package:size('img.png')").unwrap(),
        json!(PNG.len())
    );
}

#[test]
fn package_paths_stay_inside_data() {
    let mut core = Core::new(
        app_with("", &[("data/w.txt", b"x")], ""),
        Config::default(),
        Persisted::default(),
    );
    let r = core
        .lua_eval(
            "local out = {}\n\
             for _, n in ipairs({'w.txt', 'a/../w.txt', './w.txt', '../index.lua', '/w.txt', '', '.', 'nope'}) do\n\
               local v, e = ctx.package:read(n)\n\
               out[#out + 1] = v or e\n\
             end\n\
             return out",
        )
        .unwrap();
    assert_eq!(
        r,
        json!([
            "x",
            "x",
            "x",
            "invalid_argument",
            "invalid_argument",
            "invalid_argument",
            "not_found",
            "not_found"
        ])
    );
}

#[test]
fn a_decoded_png_draws_dithered_and_warns() {
    let core = Core::new(
        app_with(
            "function on_load(c) img = c.image:decode(c.package:read('img.png')) end\n\
             function on_draw(c, g) g:image(img, 10, 10) g:image(img, 20, 20, {w = 4, h = 4}) end",
            &[("data/img.png", PNG)],
            "",
        ),
        Config::default(),
        Persisted::default(),
    );
    no_errors(&core);
    assert!(core.warnings().iter().any(|w| w.contains("PNG")));
    let f = core.raw_frame();
    let px = |x, y| f.get(x, y);
    assert_eq!(
        [px(10, 10), px(11, 10), px(10, 11), px(11, 11)],
        [1, 0, 0, 1]
    );
    assert_eq!(
        [px(20, 20), px(21, 21), px(22, 20), px(22, 22)],
        [1, 1, 0, 1]
    );
}

#[test]
fn a_layer_draws_into_its_own_canvas_and_onto_the_screen() {
    let mut core = boot(
        "function on_load(c)\n\
           layer = c.layers:create(40, 30)\n\
           layer:draw(function(g) local w, h = g:size() size = w .. 'x' .. h g:rect(0, 0, 10, 10, {fill = true}) end)\n\
         end\n\
         function on_draw(c, g) g:image(layer, 100, 100) end",
    );
    no_errors(&core);
    assert_eq!(core.lua_eval("return size").unwrap(), json!("40x30"));
    let f = core.raw_frame();
    assert_eq!(
        [f.get(105, 105), f.get(115, 105), f.get(105, 125)],
        [1, 0, 0]
    );
    assert_eq!(f.get(5, 5), 0, "the layer's g must not draw on the screen");
}

#[test]
fn budget_exhaustion_inside_layer_draw_aborts_once() {
    let cfg = Config {
        instruction_budget: 1_000_000,
        ..Config::default()
    };
    let core = Core::new(
        app_with(
            "function on_load(c) local l = c.layers:create(10, 10) \
             for i = 1, 10 do l:draw(function(g) while true do end end) end after = 1 end",
            &[],
            "",
        ),
        cfg,
        Persisted::default(),
    );
    let errors = core.errors().to_vec();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].starts_with("Lua error in layer:draw callback: ")
            && errors[0].contains("instruction budget"),
        "{errors:?}"
    );
    assert!(!core.status().running);
}

#[test]
fn reader_comes_from_the_config_with_the_permission() {
    let cfg = Config {
        reader_json: Some(r#"{"current": {"title": "Идиот"}, "books": [1, 2]}"#.into()),
        ..Config::default()
    };
    let mut core = Core::new(app_with("", &[], r#""reader""#), cfg, Persisted::default());
    assert_eq!(
        core.lua_eval("local s = ctx.reader:summary() return {ctx.reader:current().title, #ctx.reader:books(), s.streak}")
            .unwrap(),
        json!(["Идиот", 2, 0])
    );
    let mut core = boot("");
    assert_eq!(
        core.lua_eval("return ctx.reader == nil").unwrap(),
        json!(true)
    );
}
