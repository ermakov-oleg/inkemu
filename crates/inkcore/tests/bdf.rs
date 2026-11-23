//! User BDF fonts: `tests/fixtures/fonts/*.bdf` are tiny synthetic fonts (hand-drawn rectangles).
use std::collections::BTreeMap;

use inkcore::draw::bdf::BdfFont;
use inkcore::draw::text::Fonts;
use inkcore::{AppFiles, Config, Core, Frame, Persisted};
use serde_json::json;

const SMALL: &[u8] = include_bytes!("../../../tests/fixtures/fonts/small.bdf");
const MEDIUM: &[u8] = include_bytes!("../../../tests/fixtures/fonts/medium.bdf");
const LARGE: &[u8] = include_bytes!("../../../tests/fixtures/fonts/large.bdf");

/// The fixture's bitmaps, as drawn by hand in the BDF files: (rows, x offset, y offset).
const A: (&[&str], i32, i32) = (
    &[
        "11111", "10001", "10001", "11111", "10001", "10001", "10001",
    ],
    1,
    0,
);
const B: (&[&str], i32, i32) = (
    &[
        "101", "010", "101", "010", "101", "010", "101", "010", "111",
    ],
    -1,
    -2,
);
const BOX: (&[&str], i32, i32) = (
    &[
        "111111", "100001", "100001", "100001", "100001", "100001", "100001", "111111",
    ],
    0,
    0,
);

fn user_fonts() -> Config {
    Config {
        fonts: Some([SMALL.to_vec(), MEDIUM.to_vec(), LARGE.to_vec()]),
        ..Config::default()
    }
}

fn boot_with(cfg: Config, index_lua: &str) -> Core {
    let manifest = r#"{"app_id": "testapp", "display_name": "Test", "entry": "index.lua", "api": "2.2", "permissions": []}"#;
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]));
    let core = Core::new(files, cfg, Persisted::default());
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
    core
}

fn draw(body: &str) -> Core {
    boot_with(
        user_fonts(),
        &format!("function on_draw(c, g) {body} end\n"),
    )
}

/// Black pixels of `f`, row by row.
fn ink(f: &Frame) -> Vec<(i32, i32)> {
    (0..f.h)
        .flat_map(|y| (0..f.w).map(move |x| (x, y)))
        .filter(|&(x, y)| f.get(x, y) == 1)
        .map(|(x, y)| (x as i32, y as i32))
        .collect()
}

/// Where a glyph lands with its pen at `pen` on the baseline `base`, as BDF places it.
fn expect(out: &mut Vec<(i32, i32)>, glyph: (&[&str], i32, i32), pen: i32, base: i32) {
    let (rows, xoff, yoff) = glyph;
    let top = base - yoff - rows.len() as i32;
    for (r, row) in rows.iter().enumerate() {
        for (c, bit) in row.chars().enumerate() {
            if bit == '1' {
                out.push((pen + xoff + c as i32, top + r as i32));
            }
        }
    }
}

fn sorted(mut v: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    v.sort_by_key(|&(x, y)| (y, x));
    v
}

#[test]
fn parses_metrics_glyphs_and_the_default_char() {
    let f = BdfFont::parse(MEDIUM).unwrap();
    assert_eq!((f.line_height, f.ascent), (24, 16));
    let a = f.glyph('A').unwrap();
    assert_eq!((a.advance, a.w, a.h, a.xoff, a.yoff), (8, 5, 7, 1, 0));
    assert!(a.ink(0, 0) && a.ink(4, 0) && !a.ink(1, 1) && a.ink(4, 6));
    // a row wider than one byte
    let w = f.glyph('W').unwrap();
    assert_eq!((w.w, w.h, w.advance), (10, 2, 11));
    let row = |r| (0..10).map(|c| u8::from(w.ink(c, r))).collect::<Vec<_>>();
    assert_eq!(row(0), [1, 1, 0, 0, 0, 0, 0, 0, 1, 1]);
    assert_eq!(row(1), [0, 0, 1, 1, 1, 1, 1, 1, 0, 0]);
    assert_eq!(f.glyph(' ').unwrap().advance, 5);
    // a codepoint the font lacks is its DEFAULT_CHAR, the "no glyph" box
    let missing = f.glyph('?').unwrap();
    assert_eq!((missing.advance, missing.w, missing.h), (9, 6, 8));
    let small = BdfFont::parse(SMALL).unwrap();
    let large = BdfFont::parse(LARGE).unwrap();
    assert_eq!(
        [
            (small.line_height, small.ascent),
            (large.line_height, large.ascent)
        ],
        [(20, 14), (30, 20)]
    );
}

#[test]
fn rejects_what_is_not_a_bdf_font() {
    assert!(BdfFont::parse(b"hello").unwrap_err().contains("STARTFONT"));
    let bad = String::from_utf8_lossy(MEDIUM).replace("BBX 5 7 1 0", "BBX five 7 1 0");
    let e = BdfFont::parse(bad.as_bytes()).unwrap_err();
    assert!(e.contains("line 24"), "{e}");
    let short = String::from_utf8_lossy(MEDIUM).replace("F8\n88\n88\nF8\n", "F8\n");
    assert!(BdfFont::parse(short.as_bytes()).is_err());
    let e = Fonts::from_bdf(&[SMALL.to_vec(), b"x".to_vec(), LARGE.to_vec()]).unwrap_err();
    assert!(e.starts_with("medium.bdf: "), "{e}");
}

#[test]
fn a_huge_glyph_box_is_an_error_not_an_allocation() {
    for bbx in ["BBX 5000 7 1 0", "BBX 5 2147483647 1 0", "BBX -5 7 1 0"] {
        let bad = String::from_utf8_lossy(MEDIUM).replace("BBX 5 7 1 0", bbx);
        let e = BdfFont::parse(bad.as_bytes()).unwrap_err();
        assert!(e.starts_with("line 24: bad BBX"), "{bbx}: {e}");
    }
}

#[test]
fn metrics_prefer_ascent_plus_descent_and_fall_back_to_the_bounding_box() {
    // PIXEL_SIZE is the em size: with FONT_ASCENT and FONT_DESCENT the line is their sum
    let em = String::from_utf8_lossy(MEDIUM).replace("PIXEL_SIZE 24", "PIXEL_SIZE 18");
    let f = BdfFont::parse(em.as_bytes()).unwrap();
    assert_eq!((f.line_height, f.ascent), (24, 16));
    // without them: ascent = box height + y offset (9 - 2), descent = -y offset (2)
    let bare = String::from_utf8_lossy(MEDIUM)
        .replace("FONT_ASCENT 16\n", "")
        .replace("FONT_DESCENT 8\n", "");
    let f = BdfFont::parse(bare.as_bytes()).unwrap();
    assert_eq!((f.line_height, f.ascent), (9, 7));
}

#[test]
fn text_is_drawn_one_to_one_from_the_bitmaps() {
    // baseline = top + ascent (16), pen advances by DWIDTH, no kerning
    let core = draw("g:text(10, 20, 'AB?')");
    let mut want = vec![];
    expect(&mut want, A, 10, 36);
    expect(&mut want, B, 18, 36);
    expect(&mut want, BOX, 23, 36);
    assert_eq!(sorted(ink(&core.raw_frame())), sorted(want));
    let t = &core.texts()[0];
    assert_eq!((t.x, t.y, t.w, t.h), (10, 20, 22, 24));
}

#[test]
fn measure_is_the_sum_of_the_bdf_advances() {
    let mut core = draw("");
    let m = core
        .lua_eval(
            "return {ctx.fonts.small:measure('AB W'), ctx.fonts.medium:measure('AB W'), \
             ctx.fonts.large:measure('AB W'), ctx.fonts.medium:measure('A?')}",
        )
        .unwrap();
    assert_eq!(
        m,
        json!([7 + 4 + 4 + 10, 8 + 5 + 5 + 11, 9 + 6 + 6 + 12, 8 + 9])
    );
    let mut core = boot_with(
        user_fonts(),
        "function on_draw(c, g) w = c.fonts.large:measure('AB W') end\n",
    );
    assert_eq!(
        core.lua_eval("return w").unwrap(),
        json!(33),
        "API 2.33.1 contract §6 says otherwise: g:measure is gone, font:measure replaces it (06/g_methods)"
    );
}

#[test]
fn ctx_fonts_metrics_come_from_the_bdf() {
    let mut core = draw("");
    let v = core
        .lua_eval(
            "local f = ctx.fonts\n\
             return {f.small.line_height, f.small.ascent, f.medium.line_height, f.medium.ascent, \
             f.large.line_height, f.large.ascent, f:size(24).name, f:size(30).name}",
        )
        .unwrap();
    assert_eq!(v, json!([20, 14, 24, 16, 30, 20, "medium", "large"]));
}

#[test]
fn text_box_wraps_by_the_bdf_widths() {
    // 'AAA AAA' is 8+8+8+5+8+8+8 = 53 wide in medium: a 30 px box wraps after the first word
    let core = draw("g:text_box(0, 0, 30, 100, 'AAA AAA')");
    let lines: Vec<_> = core.texts().iter().map(|t| (t.w, t.h)).collect();
    assert_eq!(lines, [(30, 48)]);
    let f = core.raw_frame();
    let mut want = vec![];
    for line in 0..2 {
        for i in 0..3 {
            expect(&mut want, A, i * 8, 16 + line * 24);
        }
    }
    assert_eq!(sorted(ink(&f)), sorted(want));
}

#[test]
fn layers_draw_with_the_bdf_fonts() {
    let core = boot_with(
        user_fonts(),
        "function on_load(c)\n\
           layer = c.layers:create(40, 30)\n\
           layer:draw(function(g) g:text(0, 0, 'A') end)\n\
         end\n\
         function on_draw(c, g) g:image(layer, 100, 100) end\n",
    );
    let mut want = vec![];
    expect(&mut want, A, 100, 116);
    assert_eq!(sorted(ink(&core.raw_frame())), sorted(want));
}

#[test]
fn the_lock_screen_draws_with_the_bdf_fonts() {
    let manifest = r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.2", "lockscreen": {"interval_sec": 60}}"#;
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        (
            "index.lua".to_string(),
            b"function on_draw(c, g) g:text(0, 0, 'B') end\n".to_vec(),
        ),
        (
            "lockscreen.lua".to_string(),
            b"function on_draw(c, g) g:text(50, 50, 'A') end\n".to_vec(),
        ),
    ]));
    let mut core = Core::new(files, user_fonts(), Persisted::default());
    core.lock();
    core.wait(61_000).unwrap();
    assert_eq!(core.lock_wakes(), 1);
    let mut want = vec![];
    expect(&mut want, A, 50, 66);
    assert_eq!(sorted(ink(&core.raw_frame())), sorted(want));
}

#[test]
fn unreadable_user_fonts_fall_back_to_dejavu_with_a_warning() {
    let cfg = Config {
        fonts: Some([SMALL.to_vec(), b"nope".to_vec(), LARGE.to_vec()]),
        ..Config::default()
    };
    let mut core = boot_with(cfg, "function on_draw(c, g) end\n");
    assert!(
        core.warnings()
            .iter()
            .any(|w| w.starts_with("fonts: medium.bdf: ") && w.ends_with("DejaVu Sans")),
        "{:?}",
        &*core.warnings()
    );
    let dejavu = Fonts::new();
    assert_eq!(
        core.lua_eval("return ctx.fonts.medium.ascent").unwrap(),
        json!(dejavu.ascent("medium"))
    );
}

#[test]
fn measure_and_layout_take_a_string_or_a_number_only() {
    let mut core = draw("");
    for (call, name) in [("measure(%)", "measure"), ("layout(%, 100, 100)", "layout")] {
        for (bad, ty) in [("nil", "nil"), ("{}", "table"), ("true", "boolean")] {
            let code = format!(
                "local ok, e = pcall(function() return ctx.fonts.medium:{} end) return {{ok, e}}",
                call.replace('%', bad)
            );
            let r = core.lua_eval(&code).unwrap();
            let msg = r[1].as_str().unwrap_or_default();
            assert!(
                r[0] == json!(false)
                    && msg.contains(&format!(
                        "bad argument #1 to '{name}' (string expected, got {ty})"
                    )),
                "{call} {bad}: {r}"
            );
        }
    }
    assert_eq!(
        core.lua_eval("return ctx.fonts.medium:measure(42) == ctx.fonts.medium:measure('42')")
            .unwrap(),
        json!(true)
    );
    assert_eq!(
        core.lua_eval("return select('#', ctx.fonts.medium:layout(42, 100, 100))")
            .unwrap(),
        json!(3)
    );
}
