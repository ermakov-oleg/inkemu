use std::collections::BTreeMap;

use inkcore::draw::text::Fonts;
use inkcore::draw::Canvas;
use inkcore::{AppFiles, Config, Core, Frame, Persisted};

fn boot(index_lua: &str) -> Core {
    let manifest = r#"{"app_id": "testapp", "display_name": "Test", "entry": "index.lua", "api": "2.2", "permissions": []}"#;
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index_lua.as_bytes().to_vec()),
    ]));
    Core::new(files, Config::default(), Persisted::default())
}

fn draw(body: &str) -> Core {
    let core = boot(&format!("function on_draw(c, g) {body} end\n"));
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
    core
}

fn ink_cols(f: &Frame, top: u32, bottom: u32) -> Vec<u32> {
    (0..f.w)
        .filter(|&x| (top..bottom.min(f.h)).any(|y| f.get(x, y) == 1))
        .collect()
}

fn black(f: &Frame) -> usize {
    f.px.iter().filter(|&&p| p == 1).count()
}

#[test]
fn measure_matches_drawn_advance() {
    let fonts = Fonts::new();
    for font in ["small", "medium", "large"] {
        for s in ["Tap me", "Привет", "İstanbul"] {
            let mut f = Frame::new(480, 120);
            fonts.draw(&mut f, font, 10, 10, &format!("|{s}|"));
            fonts.draw(&mut f, font, 10, 60, "|");
            let bar = ink_cols(&f, 60, 100);
            let cols = ink_cols(&f, 10, 50);
            let bar_w = bar[bar.len() - 1] - bar[0];
            let drawn = (cols[cols.len() - 1] - bar_w - cols[0]) as i32;
            let measured = fonts.measure(font, &format!("|{s}"));
            assert!(
                (drawn - measured).abs() <= 1,
                "{font} {s}: drawn {drawn}, measured {measured}"
            );
            assert!(fonts.measure(font, s) > 0);
        }
    }
}

#[test]
fn glyph_size_follows_the_line_height() {
    let fonts = Fonts::new();
    assert_eq!(
        ["small", "medium", "large", "huge"].map(|f| fonts.line_height(f)),
        [20, 24, 30, 24]
    );
    assert!(fonts.measure("large", "Hi") > fonts.measure("small", "Hi"));
}

#[test]
fn unknown_glyph_draws_nothing_but_advances() {
    let fonts = Fonts::new();
    let unknown = "\u{e000}";
    assert!(fonts.measure("medium", unknown) > 0);
    let mut f = Frame::new(100, 40);
    fonts.draw(&mut f, "medium", 10, 10, unknown);
    assert_eq!(black(&f), 0);
}

#[test]
fn rect_fill_is_black() {
    let core = draw("g:clear(0); g:rect(10, 10, 20, 20, {fill=true}); g:rect(50, 50, 20, 20, {fill=true, color=0})");
    let f = core.raw_frame();
    assert_eq!((f.w, f.h), (480, 800));
    assert_eq!(
        (f.get(15, 15), f.get(5, 5), f.get(29, 29), f.get(30, 30)),
        (1, 0, 1, 0)
    );
    assert_eq!(f.get(55, 55), 0);
}

#[test]
fn command_cap_drops_and_warns() {
    let core = draw("for i = 0, 1099 do g:rect(i % 400, i // 400, 1, 1, {fill=true}) end");
    assert_eq!(core.frame_commands(), 1100);
    assert_eq!(black(&core.raw_frame()), 1024);
    let w = core.warnings();
    assert_eq!(
        &*w,
        ["1100 draw commands in one frame; the device drops everything after 1024"]
    );
}

#[test]
fn pattern_rect_is_checker() {
    let f = draw("g:pattern_rect(0, 0, 10, 10)").raw_frame();
    let px: Vec<u8> = (0..10)
        .flat_map(|y| (0..10).map(move |x| (x, y)))
        .map(|(x, y)| f.get(x, y))
        .collect();
    assert_eq!(px.iter().filter(|&&p| p == 1).count(), 50);
    assert_ne!(f.get(0, 0), f.get(1, 0));
}

#[test]
fn text_box_wraps() {
    let core = draw(
        "g:text_box(10, 10, 100, 200, 'one two three four five six seven eight', {font='small'})",
    );
    let t = &core.texts()[0];
    assert!(
        t.text.starts_with("one two") && t.w <= 100 && t.h > 20,
        "{t:?}"
    );
    let f = core.raw_frame();
    assert!((30..50).any(|y| (10..110).any(|x| f.get(x, y) == 1)));
}

#[test]
fn wrap_splits_words_and_long_words() {
    let (c, fonts) = (Canvas::new(480, 800), Fonts::new());
    let lines = c.wrap("small", "aaaaaaaaaaaaaaaaaaaa b\nc", 60.0);
    assert!(
        lines.len() > 3 && lines[lines.len() - 1] == "c",
        "{lines:?}"
    );
    assert!(
        lines.iter().all(|l| fonts.measure("small", l) <= 60),
        "{lines:?}"
    );
    assert_eq!(lines.concat().replace(' ', ""), "aaaaaaaaaaaaaaaaaaaabc");
}

#[test]
fn huge_values_are_clipped_not_looped() {
    let core = draw(
        "g:rect(-1e9, -1e9, 2e9, 2e9); g:circle(240, 400, 1e9, {fill=true}); g:line(-1e9, 0, 1e9, 799, {width=3})
         g:arc(0, 0, 1e9, 0, 90); g:round_rect(0, 0, 1e9, 1e9, 1e9, {fill=true}); g:pattern_rect(-1e9, -1e9, 2e9, 2e9)
         g:polygon({-1e9, 0, 1e9, 0, 0, 1e9}, {fill=true}); g:text(1e30, 0, 'ab'); g:text(0, math.maxinteger, 'a')
         g:text_box(0, 1e30, 100, 100, 'a b'); g:text(-1e30, -1e30, 'ab', {align='right', valign='bottom'})",
    );
    assert!(black(&core.raw_frame()) > 0);
}

#[test]
fn g_outside_on_draw_changes_nothing_until_the_next_frame() {
    let mut core = draw("G = g; g:text(0, 0, msg or 'a')");
    core.lua_exec("G:text(0, 40, 'late'); G:rect(0, 100, 50, 50, {fill=true})")
        .unwrap();
    assert_eq!(core.texts().len(), 1);
    assert_eq!(
        black(&core.raw_frame()),
        black(&draw("g:text(0, 0, 'a')").raw_frame())
    );
    core.lua_exec("msg = 'b'; ctx:invalidate()").unwrap();
    core.wait(0).unwrap();
    let words: Vec<String> = core.texts().into_iter().map(|t| t.text).collect();
    assert_eq!(words, ["b"]);
}

#[test]
fn bad_option_values_fail_the_app() {
    for body in ["g:text(1/0, 0, 'x')", "g:line(0, 0, 5, 5, {width = 'abc'})"] {
        let core = boot(&format!("function on_draw(c, g) {body} end\n"));
        let errors = core.errors();
        assert!(
            errors[0].starts_with("g: bad values in on_draw: "),
            "{errors:?}"
        );
        assert!(!core.status().running);
        assert!(core.texts().iter().any(|t| t.text == "App error, closed"));
    }
}

#[test]
fn rtl_text_warns_and_is_skipped() {
    let core = draw("g:text(10, 10, 'שלום'); g:text(10, 40, '٣ ok')");
    assert_eq!(core.texts().len(), 1);
    assert_eq!(
        &*core.warnings(),
        ["g:text with RTL text is not drawn by the device: 'שלום'"]
    );
}

#[test]
fn error_overlay_is_a_plate_in_frame_and_texts() {
    let core =
        boot("function on_draw(c, g) g:clear(0); g:text(0, 0, 'before'); error('kaput') end\n");
    let words: Vec<String> = core.texts().into_iter().map(|t| t.text).collect();
    assert_eq!(words[0], "before");
    assert_eq!(words[1], "App error, closed");
    assert!(words[2].starts_with("Lua error in on_draw:"), "{words:?}");
    let (raw, shot) = (core.raw_frame(), core.frame(true));
    assert_ne!(raw, shot);
    // the plate's 3 px border spans x 16..w-17
    let title = &core.texts()[1];
    let y = (title.y - 10) as u32;
    assert_eq!(
        (
            shot.get(15, y),
            shot.get(16, y),
            shot.get(18, y),
            shot.get(19, y)
        ),
        (0, 1, 1, 0)
    );
}
