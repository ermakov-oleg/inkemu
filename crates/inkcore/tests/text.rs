//! API 2.33 §7 text: widths, line breaking, overflow and the styles. Expected widths come from the font.
use std::collections::BTreeMap;

use inkcore::draw::text::{Fonts, LayoutOpts, Overflow, TextStyle, Wrap};
use inkcore::{AppFiles, Config, Core, Frame, Persisted};
use serde_json::{json, Value};

const SMALL: &[u8] = include_bytes!("../../../tests/fixtures/fonts/small.bdf");
const MEDIUM: &[u8] = include_bytes!("../../../tests/fixtures/fonts/medium.bdf");
const LARGE: &[u8] = include_bytes!("../../../tests/fixtures/fonts/large.bdf");

fn boot_api(api: &str, index_lua: &str) -> Core {
    let manifest = format!(
        r#"{{"app_id": "testapp", "entry": "index.lua", "api": "{api}", "permissions": []}}"#
    );
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.into_bytes()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]));
    let core = Core::new(files, Config::default(), Persisted::default());
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
    core
}

fn draw(body: &str) -> Core {
    boot_api("2.33", &format!("function on_draw(c, g) {body} end\n"))
}

fn eval(core: &mut Core, code: &str) -> Value {
    core.lua_eval(code)
        .unwrap_or_else(|e| panic!("{code}: {e}"))
}

fn opts(wrap: Wrap, overflow: Overflow) -> LayoutOpts {
    LayoutOpts {
        wrap,
        overflow,
        ..LayoutOpts::default()
    }
}

fn texts(lay: &inkcore::draw::text::Layout) -> Vec<&str> {
    lay.lines.iter().map(|l| l.text.as_str()).collect()
}

fn ink_rows(f: &Frame, x0: u32, x1: u32) -> Vec<u32> {
    (0..f.h)
        .filter(|&y| (x0..x1.min(f.w)).any(|x| f.get(x, y) == 1))
        .collect()
}

#[test]
fn ascents_are_the_devices() {
    let f = Fonts::new();
    assert_eq!(
        ["small", "medium", "large"].map(|n| (f.line_height(n), f.ascent(n))),
        [(20, 17), (24, 21), (30, 26)]
    );
}

#[test]
fn control_characters_measure_zero_and_tab_as_space() {
    let f = Fonts::new();
    assert_eq!(f.measure("medium", "a\tb"), f.measure("medium", "a b"));
    assert_eq!(
        f.measure("medium", "a\nb\u{1}c\u{7f}"),
        f.measure("medium", "abc")
    );
    // drawn as measured: the control characters leave no ink and no gap
    let (mut a, mut b) = (Frame::new(200, 40), Frame::new(200, 40));
    f.draw(&mut a, "medium", 0, 0, "a\nb", 1);
    f.draw(&mut b, "medium", 0, 0, "ab", 1);
    assert_eq!(a, b);
}

#[test]
fn bold_adds_a_pixel_and_tabular_takes_the_widest_digit() {
    let f = Fonts::new();
    let bold = TextStyle {
        bold: true,
        ..TextStyle::default()
    };
    let tab = TextStyle {
        tabular: true,
        ..TextStyle::default()
    };
    for font in ["small", "medium", "large"] {
        let s = "Hello, 0123";
        assert_eq!(f.measure_with(font, s, bold), f.measure(font, s) + 1);
        assert_eq!(f.measure_with(font, "", bold), 0);
        let widest = ('0'..='9')
            .map(|d| f.measure(font, &d.to_string()))
            .max()
            .unwrap();
        assert_eq!(f.measure_with(font, "1111", tab), 4 * widest);
        assert_eq!(
            f.measure_with(font, "a1", tab),
            f.measure(font, "a") + widest
        );
    }
}

#[test]
fn bold_ink_is_one_pixel_wider() {
    let f = Fonts::new();
    let cols = |style: TextStyle| {
        let mut fr = Frame::new(200, 40);
        let paint = inkcore::draw::text::Paint {
            ink: 1,
            style,
            clip: None,
        };
        f.draw_styled(&mut fr, "medium", (10, 0), "Hi", paint);
        let xs: Vec<u32> = (0..fr.w)
            .filter(|&x| (0..fr.h).any(|y| fr.get(x, y) == 1))
            .collect();
        (xs[0], xs[xs.len() - 1])
    };
    let (plain, bold) = (
        cols(TextStyle::default()),
        cols(TextStyle {
            bold: true,
            ..TextStyle::default()
        }),
    );
    assert_eq!((bold.0, bold.1), (plain.0, plain.1 + 1));
}

#[test]
fn word_wrap_breaks_at_spaces_and_lines_fit() {
    let f = Fonts::new();
    let s = "alpha beta gamma delta epsilon";
    let w = f.measure("medium", "alpha beta");
    let lay = f.layout("medium", s, f64::from(w), 1000.0, &LayoutOpts::default());
    assert_eq!(texts(&lay), ["alpha beta", "gamma", "delta", "epsilon"]);
    assert!(lay.lines.iter().all(|l| l.width <= w), "{lay:?}");
    assert_eq!((lay.height, lay.consumed), (4 * 24, s.len()));
}

#[test]
fn a_word_wider_than_the_box_breaks_at_a_code_point() {
    let f = Fonts::new();
    let w = f.measure("small", "aaaa");
    let lay = f.layout(
        "small",
        "aaaaaaaaaa b",
        f64::from(w),
        1000.0,
        &LayoutOpts::default(),
    );
    assert_eq!(texts(&lay), ["aaaa", "aaaa", "aa b"]);
}

#[test]
fn char_wrap_fills_the_line_and_none_breaks_only_at_newlines() {
    let f = Fonts::new();
    let w = f.measure("medium", "alpha be");
    let lay = f.layout(
        "medium",
        "alpha beta",
        f64::from(w),
        1000.0,
        &opts(Wrap::Char, Overflow::Clip),
    );
    assert_eq!(texts(&lay), ["alpha be", "ta"]);
    let lay = f.layout(
        "medium",
        "alpha beta\ngamma",
        10.0,
        1000.0,
        &opts(Wrap::None, Overflow::Visible),
    );
    assert_eq!(texts(&lay), ["alpha beta", "gamma"]);
}

#[test]
fn char_wrap_keeps_combining_marks_with_their_base() {
    let f = Fonts::new();
    let s = "e\u{301}e\u{301}e\u{301}e\u{301}";
    let w = f.measure("medium", "e\u{301}e");
    let lay = f.layout(
        "medium",
        s,
        f64::from(w),
        1000.0,
        &opts(Wrap::Char, Overflow::Clip),
    );
    assert!(
        lay.lines.iter().all(|l| !l.text.starts_with('\u{301}')),
        "{lay:?}"
    );
    assert_eq!(lay.lines.concat_text(), s);
}

trait Concat {
    fn concat_text(&self) -> String;
}

impl Concat for Vec<inkcore::draw::text::Line> {
    fn concat_text(&self) -> String {
        self.iter().map(|l| l.text.as_str()).collect()
    }
}

#[test]
fn word_wrap_breaks_beside_cjk_and_keeps_closing_punctuation_on_the_line() {
    let f = Fonts::new();
    let s = "\u{4E2D}\u{6587}\u{5B57}\u{3002}";
    let w = f.measure("medium", "\u{4E2D}\u{6587}\u{5B57}");
    let old = f.layout("medium", s, f64::from(w), 1000.0, &LayoutOpts::default());
    assert_eq!(texts(&old), ["\u{4E2D}\u{6587}\u{5B57}", "\u{3002}"]);
    // 2.16: 。 may not start a line, so 字 moves down with it
    let o = LayoutOpts {
        kinsoku: true,
        ..LayoutOpts::default()
    };
    let new = f.layout("medium", s, f64::from(w), 1000.0, &o);
    assert_eq!(texts(&new), ["\u{4E2D}\u{6587}", "\u{5B57}\u{3002}"]);
}

#[test]
fn bold_wraps_one_pixel_earlier() {
    let f = Fonts::new();
    let w = f64::from(f.measure("medium", "ab cd"));
    let plain = f.layout("medium", "ab cd", w, 1000.0, &LayoutOpts::default());
    let mut o = LayoutOpts::default();
    o.style.bold = true;
    let bold = f.layout("medium", "ab cd", w, 1000.0, &o);
    assert_eq!((plain.lines.len(), bold.lines.len()), (1, 2));
    assert!(bold.lines.iter().all(|l| f64::from(l.width) <= w));
}

#[test]
fn clip_keeps_the_lines_that_fit_and_counts_their_bytes() {
    let f = Fonts::new();
    let s = "one\ntwo\nthree";
    let lay = f.layout(
        "medium",
        s,
        400.0,
        2.0 * 24.0 + 23.0,
        &LayoutOpts::default(),
    );
    assert_eq!(
        (texts(&lay), lay.height, lay.consumed),
        (vec!["one", "two"], 48, 8)
    );
    let o = LayoutOpts {
        line_gap: 5,
        ..LayoutOpts::default()
    };
    // two lines need 24 + 5 + 24
    assert_eq!(f.layout("medium", s, 400.0, 52.0, &o).lines.len(), 1);
    let lay = f.layout("medium", s, 400.0, 53.0, &o);
    assert_eq!((lay.lines.len(), lay.height), (2, 53));
    assert_eq!(f.layout("medium", s, 400.0, 23.0, &o), Default::default());
}

#[test]
fn visible_keeps_every_line() {
    let f = Fonts::new();
    let lay = f.layout(
        "medium",
        "a\nb\nc\n",
        400.0,
        1.0,
        &opts(Wrap::Word, Overflow::Visible),
    );
    // a newline at the very end does not start another line
    assert_eq!((texts(&lay), lay.consumed), (vec!["a", "b", "c"], 6));
}

#[test]
fn a_space_at_the_wrap_point_belongs_to_the_line_before() {
    let f = Fonts::new();
    let w = f.measure("medium", "alpha");
    let lay = f.layout(
        "medium",
        "alpha   beta gamma",
        f64::from(w),
        24.0,
        &LayoutOpts::default(),
    );
    assert_eq!((texts(&lay), lay.consumed), (vec!["alpha"], 8));
}

#[test]
fn ellipsis_ends_the_last_line_that_fits() {
    let f = Fonts::new();
    let w = f.measure("medium", "alpha beta");
    let o = opts(Wrap::Word, Overflow::Ellipsis);
    // the second line is filled by a broken word, so the ellipsis takes characters off it
    let s = "alpha beta gammagammagamma delta";
    let lay = f.layout("medium", s, f64::from(w), 2.0 * 24.0, &o);
    assert_eq!(lay.lines.len(), 2);
    let last = &lay.lines[1].text;
    assert!(last.ends_with('…') && last.starts_with("gam"), "{lay:?}");
    assert!(lay.lines[1].width <= w);
    assert_eq!(lay.lines[1].width, f.measure("medium", last));
    // the next page starts where the shortened line stopped
    let kept = last.trim_end_matches('…');
    assert_eq!(&s[..lay.consumed], &format!("alpha beta {kept}"));
    // nothing taken off: the wrap-point space still counts toward the page
    let s = "alpha beta gamma delta";
    let w = w.max(f.measure("medium", "gamma…"));
    assert!(f.measure("medium", "gamma delta") > w && f.measure("medium", "alpha beta gamma") > w);
    let lay = f.layout("medium", s, f64::from(w), 2.0 * 24.0, &o);
    assert_eq!(texts(&lay), ["alpha beta", "gamma…"]);
    assert_eq!(&s[..lay.consumed], "alpha beta gamma ");
    // everything fits: no ellipsis
    let lay = f.layout("medium", "alpha", f64::from(w), 24.0, &o);
    assert_eq!(texts(&lay), ["alpha"]);
}

#[test]
fn ellipsis_shortens_a_line_that_cannot_wrap() {
    let f = Fonts::new();
    let w = f.measure("medium", "alpha be");
    let lay = f.layout(
        "medium",
        "alpha beta gamma\nx",
        f64::from(w),
        1000.0,
        &opts(Wrap::None, Overflow::Ellipsis),
    );
    assert_eq!(lay.lines.len(), 2);
    assert!(
        lay.lines[0].text.ends_with('…') && lay.lines[0].width <= w,
        "{lay:?}"
    );
    assert_eq!(lay.lines[1].text, "x");
}

#[test]
fn ellipsis_is_three_dots_in_a_font_without_one() {
    let f = Fonts::from_bdf(&[SMALL.to_vec(), MEDIUM.to_vec(), LARGE.to_vec()]).unwrap();
    let o = opts(Wrap::None, Overflow::Ellipsis);
    let lay = f.layout("medium", "AAAAAAAAAAAAAAAA", 60.0, 1000.0, &o);
    assert!(lay.lines[0].text.ends_with("..."), "{lay:?}");
    assert!(lay.lines[0].width <= 60);
}

#[test]
fn consumed_pages_through_the_text_on_code_points() {
    let f = Fonts::new();
    let s = "Hello w\u{f6}rld \u{6F22}\u{5B57} test. ".repeat(20);
    let (mut pos, mut pages) = (0, 0);
    while pos < s.len() {
        let lay = f.layout("medium", &s[pos..], 200.0, 72.0, &LayoutOpts::default());
        assert!(lay.consumed > 0 && lay.lines.len() <= 3);
        pos += lay.consumed;
        assert!(s.is_char_boundary(pos));
        pages += 1;
    }
    assert!(pages > 1);
}

#[test]
fn a_long_unbreakable_word_is_laid_out_in_linear_time() {
    let f = Fonts::new();
    let s = "a".repeat(65536);
    let t = std::time::Instant::now();
    let vis = f.layout(
        "small",
        &s,
        400.0,
        0.0,
        &opts(Wrap::Word, Overflow::Visible),
    );
    assert_eq!(vis.consumed, s.len());
    assert!(t.elapsed().as_secs() < 10, "{:?}", t.elapsed());
}

/// `(lines, height, consumed)` of `font:layout` and the commands the same `text_box` takes.
fn layout_and_box(text: &str, w: i32, h: i32, opts: &str) -> (Value, Value) {
    let mut core = draw(&format!("g:text_box(0, 0, {w}, {h}, {text:?}, {opts})"));
    let lay = eval(
        &mut core,
        &format!("return {{ctx.fonts.medium:layout({text:?}, {w}, {h}, {opts})}}"),
    );
    let cmds = eval(&mut core, "return ctx.perf:info().cmd_count");
    (lay, cmds)
}

#[test]
fn layout_counts_the_lines_text_box_draws() {
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
    for o in [
        "{}",
        "{wrap = 'char'}",
        "{wrap = 'none'}",
        "{overflow = 'ellipsis'}",
        "{overflow = 'visible'}",
        "{line_gap = 6}",
        "{bold = true, tabular = true}",
    ] {
        for (w, h) in [(100, 80), (150, 400), (60, 23), (400, 30)] {
            let (lay, cmds) = layout_and_box(text, w, h, o);
            assert_eq!(lay[0], cmds, "{o} {w}x{h}: {lay}");
        }
    }
}

#[test]
fn an_underline_is_one_more_command_per_line_and_drawn_under_the_baseline() {
    let mut core =
        draw("g:text_box(10, 0, 200, 200, 'ab\\ncd', {underline = true, strike = true})");
    assert_eq!(
        eval(&mut core, "return ctx.perf:info().cmd_count"),
        json!(6)
    );
    let mut core = draw("g:text(10, 0, 'ab', {underline = true})");
    assert_eq!(
        eval(&mut core, "return ctx.perf:info().cmd_count"),
        json!(2)
    );
    let width = eval(&mut core, "return ctx.fonts.medium:measure('ab')")
        .as_i64()
        .unwrap() as u32;
    let f = core.raw_frame();
    // medium: line height 24, ascent 21, thickness max(1, 36 // 24) = 1: the row at 22
    let row: Vec<u32> = (0..f.w).filter(|&x| f.get(x, 22) == 1).collect();
    assert_eq!(row, (10..10 + width).collect::<Vec<u32>>());
}

#[test]
fn a_strikethrough_crosses_the_middle_of_the_line_box() {
    let core = draw("g:text(10, 0, 'ab', {strike = true, color = 15})");
    let f = core.raw_frame();
    // 24 // 2 - 1 // 2 = 12
    let width = Fonts::new().measure("medium", "ab") as u32;
    assert!((10..10 + width).all(|x| f.get(x, 12) == 1));
    assert_eq!(f.get(10 + width, 12), 0);
}

#[test]
fn an_empty_line_takes_no_stroke_command() {
    let mut core = draw("g:text(10, 0, '', {underline = true})");
    assert_eq!(
        eval(&mut core, "return ctx.perf:info().cmd_count"),
        json!(1)
    );
}

#[test]
fn older_packages_ignore_the_newer_options() {
    let mut core = boot_api(
        "2.2",
        "function on_draw(c, g) g:text(10, 0, 'ab', {underline = true}) end\n",
    );
    assert_eq!(
        eval(&mut core, "return ctx.perf:info().cmd_count"),
        json!(1)
    );
    assert_eq!(
        eval(
            &mut core,
            "local f = ctx.fonts.medium return f:measure('ab', {bold = true}) - f:measure('ab')"
        ),
        json!(0)
    );
    let mut core = boot_api("2.16", "");
    assert_eq!(
        eval(
            &mut core,
            "local f = ctx.fonts.medium return f:measure('ab', {bold = true}) - f:measure('ab')"
        ),
        json!(1)
    );
}

#[test]
fn nothing_fits_draws_nothing_and_succeeds() {
    let mut core = draw("r = {g:text_box(0, 0, 200, 23, 'abc')}");
    assert_eq!(
        eval(&mut core, "return {#r, ctx.perf:info().cmd_count}"),
        json!([0, 0])
    );
    assert!(core.raw_frame().px.iter().all(|&p| p == 0));
}

#[test]
fn clip_cuts_a_line_that_cannot_wrap_at_the_box() {
    let core = draw("g:text_box(10, 0, 30, 24, 'WWWWWWWWWW', {wrap = 'none'})");
    let f = core.raw_frame();
    let xs: Vec<u32> = (0..f.w)
        .filter(|&x| (0..f.h).any(|y| f.get(x, y) == 1))
        .collect();
    assert!(xs[0] >= 10 && xs[xs.len() - 1] < 40, "{xs:?}");
    let core =
        draw("g:text_box(10, 0, 30, 24, 'WWWWWWWWWW', {wrap = 'none', overflow = 'visible'})");
    let f = core.raw_frame();
    assert!((40..f.w).any(|x| (0..24).any(|y| f.get(x, y) == 1)));
}

#[test]
fn valign_baseline_puts_the_baseline_on_y() {
    let core = draw("g:text(10, 100, 'H', {valign = 'baseline'})");
    let rows = ink_rows(&core.raw_frame(), 0, 480);
    assert_eq!(rows[rows.len() - 1], 99);
    let core = draw("g:text(10, 100 - 21, 'H')");
    assert_eq!(ink_rows(&core.raw_frame(), 0, 480), rows);
}

#[test]
fn rtl_and_invalid_utf8_are_refused_with_a_warning() {
    let mut core = draw("r1 = {g:text(10, 10, 'שלום')} r2 = {g:text_box(0, 0, 9, 9, 'a\\xffb')}");
    assert_eq!(
        eval(
            &mut core,
            "return {tostring(r1[1]), r1[2], tostring(r2[1]), r2[2]}"
        ),
        json!(["nil", "invalid_argument", "nil", "invalid_argument"])
    );
    assert!(core.texts().is_empty());
    assert_eq!(core.warnings().len(), 2, "{:?}", &*core.warnings());
}

/// A font whose `1` (advance 4, one column of ink) is narrower than its `0` (advance 8).
const DIGITS: &str = "STARTFONT 2.1
FONTBOUNDINGBOX 8 4 0 0
STARTPROPERTIES 2
FONT_ASCENT 4
FONT_DESCENT 0
ENDPROPERTIES
CHARS 2
STARTCHAR one
ENCODING 49
DWIDTH 4 0
BBX 1 4 0 0
BITMAP
80
80
80
80
ENDCHAR
STARTCHAR zero
ENCODING 48
DWIDTH 8 0
BBX 8 4 0 0
BITMAP
FF
81
81
FF
ENDCHAR
ENDFONT
";

#[test]
fn tabular_digits_take_the_widest_cell_and_are_centred_to_the_left() {
    let font = DIGITS.as_bytes().to_vec();
    let f = Fonts::from_bdf(&[font.clone(), font.clone(), font]).unwrap();
    let tab = TextStyle {
        tabular: true,
        ..TextStyle::default()
    };
    assert_eq!(f.measure("medium", "11"), 8);
    assert_eq!(f.measure_with("medium", "11", tab), 16);
    let mut fr = Frame::new(40, 4);
    let paint = inkcore::draw::text::Paint {
        ink: 1,
        style: tab,
        clip: None,
    };
    f.draw_styled(&mut fr, "medium", (0, 0), "11", paint);
    // each 1 sits at (8 - 4) // 2 = 2 in its 8-pixel cell
    let cols: Vec<u32> = (0..fr.w).filter(|&x| fr.get(x, 0) == 1).collect();
    assert_eq!(cols, [2, 10]);
}

#[test]
fn italic_slants_the_ink_and_keeps_the_width() {
    let f = Fonts::new();
    let style = TextStyle {
        italic: true,
        ..TextStyle::default()
    };
    assert_eq!(
        f.measure_with("large", "Il", style),
        f.measure("large", "Il")
    );
    let draw = |style: TextStyle| {
        let mut fr = Frame::new(100, 40);
        let paint = inkcore::draw::text::Paint {
            ink: 1,
            style,
            clip: None,
        };
        f.draw_styled(&mut fr, "large", (10, 0), "I", paint);
        fr
    };
    let (plain, slanted) = (draw(TextStyle::default()), draw(style));
    let rows = ink_rows(&plain, 0, 100);
    let left = |fr: &Frame, y: u32| (0..fr.w).find(|&x| fr.get(x, y) == 1).unwrap();
    let (top, bottom) = (rows[0], rows[rows.len() - 1]);
    // the bottom row stays, the top row moves right
    assert_eq!(left(&slanted, bottom), left(&plain, bottom));
    assert!(left(&slanted, top) > left(&plain, top));
}

#[test]
fn a_huge_box_height_keeps_every_line() {
    let f = Fonts::new();
    for h in [1e30, f64::INFINITY, f64::MAX] {
        let lay = f.layout("medium", "a\nb", 200.0, h, &LayoutOpts::default());
        assert_eq!(lay.lines.len(), 2, "{h}");
    }
    let mut core = draw("g:text_box(0, 0, 100, 1e30, 'a b')");
    assert_eq!(
        eval(
            &mut core,
            "return {ctx.fonts.medium:layout('a\\nb', 200, math.huge)}"
        ),
        json!([2, 48, 3])
    );
}

#[test]
fn a_long_line_is_shortened_with_an_ellipsis_in_linear_time() {
    let f = Fonts::new();
    let s = "a".repeat(65536);
    let t = std::time::Instant::now();
    let o = opts(Wrap::None, Overflow::Ellipsis);
    let lay = f.layout("small", &s, 100.0, 100.0, &o);
    assert!(lay.lines[0].text.ends_with('…') && lay.lines[0].width <= 100);
    assert!(t.elapsed().as_secs() < 10, "{:?}", t.elapsed());
}

#[test]
fn clip_leaves_lines_that_fit_whole() {
    // an italic line that fits is not cut at the box edge, its slanted top may overhang
    let w = Fonts::new().measure("large", "I");
    let core = draw(&format!(
        "g:text_box(10, 0, {w}, 30, 'I', {{font = c.fonts.large, italic = true}})"
    ));
    let f = core.raw_frame();
    let right = (0..f.w)
        .filter(|&x| (0..f.h).any(|y| f.get(x, y) == 1))
        .max()
        .unwrap();
    let plain = draw(&format!(
        "g:text_box(10, 0, {w}, 30, 'I', {{font = c.fonts.large}})"
    ))
    .raw_frame();
    let plain_right = (0..plain.w)
        .filter(|&x| (0..plain.h).any(|y| plain.get(x, y) == 1))
        .max()
        .unwrap();
    assert!(right > plain_right, "{right} {plain_right}");
}

#[test]
fn a_number_sample_raises() {
    let mut core = boot_api("2.33", "");
    assert_eq!(
        eval(
            &mut core,
            "return {pcall(function() return ctx.fonts:size(24, 5) end)}"
        )[0],
        json!(false)
    );
}

#[test]
fn measure_and_layout_refuse_right_to_left_text() {
    let mut core = boot_api("2.33", "");
    assert_eq!(
        eval(
            &mut core,
            "local f = ctx.fonts.medium local a, b = f:measure('x \u{05D0}') local c, d = f:layout('\u{0627}', 100, 100) \
             return {tostring(a), b, tostring(c), d}"
        ),
        json!(["nil", "invalid_argument", "nil", "invalid_argument"])
    );
}

/// `lines, consumed` of a one-line, char-wrapped `layout` of `text` in a box as wide as `fit`.
fn char_layout(api: &str, text: &str, fit: &str) -> Value {
    let mut core = boot_api(api, "");
    eval(
        &mut core,
        &format!(
            "local f = ctx.fonts.medium local l, _, c = f:layout({text:?}, f:measure({fit:?}), f.line_height, \
             {{wrap = 'char'}}) return {{l, c}}"
        ),
    )
}

#[test]
fn kinsoku_moves_a_char_break_back_to_a_word_break_point() {
    // the break before "." is not allowed from 2.16; between "b" and "c" is no word break point, so it stays
    assert_eq!(char_layout("2.15", "abc.", "abc"), json!([1, 3]));
    assert_eq!(char_layout("2.16", "abc.", "abc"), json!([1, 3]));
    // here the word break before "cd." is within reach, so "cd" moves to the next line with the "."
    assert_eq!(char_layout("2.15", "ab cd.", "ab cd"), json!([1, 5]));
    assert_eq!(char_layout("2.16", "ab cd.", "ab cd"), json!([1, 3]));
}

#[test]
fn centred_text_starts_at_x_minus_half_the_width_floored() {
    let f = Fonts::new();
    let s = ["Hi", "Hil", "Hill", "Hello", "abc"]
        .into_iter()
        .find(|s| f.measure("medium", s) % 2 == 1)
        .expect("an odd width");
    let w = f.measure("medium", s);
    for (opts, x, y) in [
        (
            "align = 'center', valign = 'middle'",
            101 - w / 2,
            51 - 24 / 2,
        ),
        ("align = 'right', valign = 'bottom'", 101 - w, 51 - 24),
    ] {
        let core = draw(&format!("g:text(101, 51, '{s}', {{{opts}}})"));
        let t = &core.texts()[0];
        assert_eq!((t.x, t.y), (x, y), "{opts}");
    }
}
