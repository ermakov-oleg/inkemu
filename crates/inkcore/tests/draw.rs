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
            fonts.draw(&mut f, font, 10, 10, &format!("|{s}|"), 1);
            fonts.draw(&mut f, font, 10, 60, "|", 1);
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
    fonts.draw(&mut f, "medium", 10, 10, unknown, 1);
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
        ["1100 draw commands in one frame; the device refuses every call past 1024"],
        "API 2.33.1 contract §5 says otherwise: calls past the cap are refused (05/cmd_cap_1025)"
    );
}

/// Indices of the full frames among the presented ones.
fn fulls(core: &Core) -> Vec<usize> {
    let st = core.status();
    assert_eq!(st.refresh, st.refreshes.last().copied());
    (0..st.refreshes.len())
        .filter(|&i| st.refreshes[i] == "full")
        .collect()
}

#[test]
fn a_deferral_holds_the_automatic_full_until_it_ends() {
    let mut core = boot(
        "function on_load(c) c:defer_full_refresh(500) c:set_tick(10) end
         function on_tick(c) c:invalidate() end
         function on_draw(c, g) end
",
    );
    core.advance(490).unwrap();
    // frame 30 would be the automatic full; the first one at 500 ms gets it
    assert_eq!(fulls(&core), [0]);
    assert_eq!(core.status().refreshes.len(), 50);
    core.advance(10).unwrap();
    assert_eq!(core.status().refresh, Some("full"));
}

#[test]
fn a_renewed_deferral_still_gets_a_full_at_twice_the_interval() {
    let mut core = boot(
        "function on_load(c) c:set_tick(10) end
         function on_tick(c) c:defer_full_refresh(60000) c:invalidate() end
         function on_draw(c, g) end
",
    );
    core.advance(590).unwrap();
    assert_eq!(fulls(&core), [0]);
    for (ms, kind) in [(10, "full"), (590, "partial"), (10, "full")] {
        core.advance(ms).unwrap();
        assert_eq!(core.status().refresh, Some(kind));
    }
}

#[test]
fn refreshes_keep_the_last_64_frames() {
    let mut core = boot(
        "local n = 0\n\
         function on_load(c) c:set_tick(10) end\n\
         function on_tick(c) n = n + 1 if n == 99 then c:invalidate('full') else c:invalidate() end end\n\
         function on_draw(c, g) end\n",
    );
    core.advance(990).unwrap();
    // frames 0..=99: automatic fulls at 30, 60 and 90, the requested one at 99; the window starts at frame 36
    assert_eq!(core.status().refreshes.len(), 64);
    assert_eq!(fulls(&core), [24, 54, 63]);
}

#[test]
fn failed_frames_are_not_counted_and_a_restart_starts_full() {
    let mut core = boot(
        "function on_input(c, ev) if ev.phase == 'up' then boom = true c:invalidate() end return true end
         function on_draw(c, g) if boom then error('boom') end end
",
    );
    core.tap(1, 1).unwrap();
    assert_eq!(core.errors().len(), 1);
    assert_eq!(core.status().refreshes, ["full"]);
    core.restart();
    assert_eq!(core.status().refreshes, ["full", "full"]);
}

#[test]
fn perf_counts_the_screen_frame_only() {
    let mut core = boot(
        "function on_load(c)
           l = c.layers:create(10, 10)
           l:draw(function(g) for i = 1, 5 do g:rect(0, 0, 1, 1) end end)
         end
         function on_draw(c, g) g:rect(0, 0, 1, 1) g:image(l, 0, 0) g:text_box(0, 0, 400, 100, 'a\\nb') end
",
    );
    let p = core
        .lua_eval("local p = ctx.perf:info() return {p.cmd_count, p.image_count, p.draw_frame}")
        .unwrap();
    // the two-line text box is two commands
    assert_eq!(p, serde_json::json!([4, 1, 1]));
}

#[test]
fn the_0_1_extras_of_g_are_gone() {
    let mut core = draw("G = g");
    let gone = core
        .lua_eval("return G.round_rect == nil and G.pattern_rect == nil and G.fill_pattern == nil")
        .unwrap();
    assert_eq!(
        gone,
        serde_json::json!(true),
        "API 2.33.1 contract §6 says otherwise: firmware 26112001 has no g:pattern_rect (06/g_methods)"
    );
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
    let (c, fonts) = (Canvas::new(480, 800, Fonts::new()), Fonts::new());
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
         g:arc(0, 0, 1e9, 0, 90); g:rect(0, 0, 1e9, 1e9, {radius=1e9, fill=true}); g:pattern(-1e9, -1e9, 2e9, 2e9)
         g:polygon({-1e9, 0, 1e9, 0, 0, 1e9}, {fill=true}); g:text(1e30, 0, 'ab'); g:text(0, math.maxinteger, 'a')
         g:text_box(0, 1e30, 100, 100, 'a b'); g:text(-1e30, -1e30, 'ab', {align='right', valign='bottom'})",
    );
    assert!(
        black(&core.raw_frame()) > 0,
        "API 2.33.1 contract §6 says otherwise: round_rect and pattern_rect are gone (06/g_methods)"
    );
}

#[test]
fn a_layer_g_kept_after_its_draw_is_inactive() {
    let mut core = boot(
        "function on_load(c) l = c.layers:create(10, 10) l:draw(function(g) kept = g end) end\n\
         function on_draw(c, g) end\n",
    );
    let r = core
        .lua_eval("local a, b = kept:rect(0, 0, 5, 5) local c, d = kept:size() return {a == nil, b, c == nil, d}")
        .unwrap();
    assert_eq!(r, serde_json::json!([true, "inactive", true, "inactive"]));
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

/// `g:image` of a 20×10 layer, drawn by `layer` in `on_load`, onto a white screen by `body`.
fn layer_frame(layer: &str, body: &str) -> Frame {
    let core = boot(&format!(
        "function on_load(c) l = c.layers:create(20, 10) l:draw(function(g) g:clear(0) {layer} end) end\n\
         function on_draw(c, g) g:clear(0) {body} end\n"
    ));
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
    core.raw_frame()
}

/// A 20×10 layer, black on its left half.
fn half_black_layer(body: &str) -> Frame {
    layer_frame("g:rect(0, 0, 10, 10, {fill=true})", body)
}

fn row(f: &Frame, x: u32, y: u32, n: u32) -> Vec<u8> {
    (x..x + n).map(|x| f.get(x, y)).collect()
}

#[test]
fn image_src_draws_only_its_region_named_or_positional() {
    let f = half_black_layer(
        "g:image(l, 100, 100, {src = {x = 5, y = 0, w = 10, h = 10}})
         g:image(l, 100, 200, {src = {5, 0, 10, 10}})",
    );
    for y in [100, 109, 200, 209] {
        assert_eq!(
            row(&f, 99, y, 12),
            [0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0],
            "row {y}"
        );
    }
    for y in [99, 110, 199, 210] {
        assert_eq!(row(&f, 99, y, 12), [0; 12], "row {y}");
    }
}

#[test]
fn image_src_is_scaled_to_w_and_h() {
    // 8 black and 2 white columns, doubled
    let f = half_black_layer(
        "g:image(l, 100, 100, {src = {x = 2, y = 0, w = 10, h = 10}, w = 20, h = 20})",
    );
    let want: Vec<u8> = (0..22).map(|i| u8::from((1..17).contains(&i))).collect();
    for y in [100, 119] {
        assert_eq!(row(&f, 99, y, 22), want, "row {y}");
    }
    assert_eq!(row(&f, 99, 120, 22), [0; 22]);
}

#[test]
fn image_invert_swaps_black_and_white_of_the_region_only() {
    let f = half_black_layer(
        "g:image(l, 100, 100, {invert = true})
         g:image(l, 100, 200, {src = {x = 5, y = 0, w = 10, h = 10}, invert = true})",
    );
    let mut want = vec![0];
    want.extend([0; 10]);
    want.extend([1; 10]);
    want.push(0);
    assert_eq!(row(&f, 99, 105, 22), want);
    assert_eq!(row(&f, 99, 205, 12), [0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0]);
    assert_eq!(row(&f, 99, 210, 12), [0; 12]);
}

#[test]
fn image_radius_clips_like_a_rounded_rect_and_white_color_tints_ink() {
    let black_layer = |body: &str| layer_frame("g:clear(1)", body);
    let clipped = black_layer("g:image(l, 30, 30, {radius = 4})");
    let rect = black_layer("g:rect(30, 30, 20, 10, {radius = 4, fill = true})");
    assert_eq!(clipped, rect);
    assert_eq!((clipped.get(30, 30), clipped.get(34, 34)), (0, 1));
    assert_eq!(black(&black_layer("g:image(l, 30, 30, {color = 0})")), 0);
    assert_eq!(black(&black_layer("g:image(l, 30, 30, {color = 8})")), 200);
}

#[test]
fn image_transform_options_warn_once_and_draw_untransformed() {
    let core = boot(
        "function on_load(c) l = c.layers:create(20, 10) l:draw(function(g) g:clear(0) g:rect(0, 0, 10, 10, {fill=true}) end) end\n\
         function on_draw(c, g) g:clear(0) g:image(l, 0, 0, {flip_x = true}) g:image(l, 0, 20, {rotate = 90}) g:image(l, 0, 40) end\n",
    );
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
    let f = core.raw_frame();
    assert_eq!((f.get(2, 5), f.get(15, 5)), (1, 0));
    let w: Vec<_> = core
        .warnings()
        .iter()
        .filter(|w| w.contains("not emulated"))
        .cloned()
        .collect();
    assert_eq!(w.len(), 1, "{w:?}");
}
