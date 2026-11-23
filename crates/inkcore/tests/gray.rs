//! Grey on the 1-bit canvas as measured on firmware 26112001 (`research-26112001/gray.md`): `color` is a
//! threshold and `g:pattern` dithers. Gray frames (`g:image{gray = true}`) follow the API 2.33.1 contract §3.
use std::collections::BTreeMap;
use std::io::Cursor;

use inkcore::{AppFiles, Config, Core, Frame, Persisted};

/// The standard 8×8 Bayer matrix; the device's `dot` pattern matched it pixel for pixel, anchored to the screen.
const BAYER: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

fn draw(body: &str) -> Core {
    let manifest = r#"{"app_id": "testapp", "display_name": "Test", "entry": "index.lua", "api": "2.2", "permissions": []}"#;
    let index = format!("function on_draw(c, g) {body} end\n");
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index.into_bytes()),
    ]));
    let core = Core::new(files, Config::default(), Persisted::default());
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
    core
}

fn black_in(f: &Frame, x: u32, y: u32, w: u32, h: u32) -> usize {
    (y..y + h)
        .flat_map(|py| (x..x + w).map(move |px| (px, py)))
        .filter(|&(px, py)| f.get(px, py) == 1)
        .count()
}

#[test]
fn color_0_to_7_draws_white_and_8_to_15_black() {
    // 0.1 drew every colour but 0 black
    let why = "reason (d): gray.md lines 21-22 and table row 1: 0-7 draw white, 8-15 black";
    for (c, ink) in [(0, 0), (1, 0), (7, 0), (8, 1), (9, 1), (15, 1)] {
        let on_white = draw(&format!(
            "g:rect(10, 10, 20, 20, {{fill = true, color = {c}}})\n\
             g:line(40, 10, 40, 30, {{color = {c}}})\n\
             g:circle(70, 20, 8, {{fill = true, color = {c}}})\n\
             g:polygon({{100, 10, 120, 10, 120, 30, 100, 30}}, {{fill = true, color = {c}}})\n\
             g:text(10, 50, 'MMMM', {{font = 'large', color = {c}}})"
        ))
        .raw_frame();
        let on_black = draw(&format!(
            "g:clear(1)\n\
             g:rect(10, 10, 20, 20, {{fill = true, color = {c}}})\n\
             g:line(40, 10, 40, 30, {{color = {c}}})\n\
             g:circle(70, 20, 8, {{fill = true, color = {c}}})\n\
             g:polygon({{100, 10, 120, 10, 120, 30, 100, 30}}, {{fill = true, color = {c}}})\n\
             g:text(10, 50, 'MMMM', {{font = 'large', color = {c}}})"
        ))
        .raw_frame();
        assert_eq!(
            on_black.get(300, 300),
            1,
            "g:clear(1) must give a black background"
        );
        let f = if ink == 1 { &on_white } else { &on_black };
        let probes = [(20, 20), (40, 20), (70, 20), (110, 20)];
        for (x, y) in probes {
            assert_eq!(f.get(x, y), ink, "color={c} at ({x}, {y}); {why}");
        }
        let text_ink = black_in(f, 0, 40, 200, 50);
        let text_paper = 200 * 50 - text_ink;
        if ink == 1 {
            assert!(text_ink > 50, "color={c}: text should be black; {why}");
        } else {
            assert!(text_paper > 50, "color={c}: text should be white; {why}");
        }
    }
}

#[test]
fn color_without_a_number_keeps_the_0_1_meaning() {
    let f = draw(
        "g:rect(20, 0, 10, 10, {fill = true}); g:rect(40, 0, 10, 10, {fill = true, color = true})",
    )
    .raw_frame();
    assert_eq!([f.get(25, 5), f.get(45, 5)], [1, 1]);
    let f = draw("g:clear(1); g:rect(0, 0, 10, 10, {fill = true, color = false})").raw_frame();
    assert_eq!([f.get(5, 5), f.get(20, 20)], [0, 1]);
}

#[test]
fn dot_pattern_inks_the_density_share_through_a_screen_anchored_bayer_matrix() {
    let f = draw("g:pattern(48, 48, 64, 64, {kind = 'dot', density = 25, color = 15})").raw_frame();
    // reason: gray.md "density = процент чернил (... 25 → 25% ...)"
    assert_eq!(black_in(&f, 48, 48, 64, 64), 64 * 64 / 4);
    assert_eq!(
        black_in(&f, 0, 0, 480, 800),
        64 * 64 / 4,
        "nothing outside the box"
    );
    // the device's measured shares per 8×8 tile: 10 → 6, 38 → 24, 69 → 44, 80 → 51, 95 → 61 (2-patterns.png)
    for (density, per_tile) in [
        (10, 6),
        (25, 16),
        (38, 24),
        (50, 32),
        (69, 44),
        (80, 51),
        (95, 61),
        (0, 0),
        (100, 64),
    ] {
        // an odd origin: the matrix stays anchored to the screen, not to the box
        let f = draw(&format!(
            "g:pattern(3, 5, 64, 64, {{kind = 'dot', density = {density}, color = 15}})"
        ))
        .raw_frame();
        for y in 5..69u32 {
            for x in 3..67u32 {
                let want = u8::from(BAYER[(y % 8) as usize][(x % 8) as usize] < per_tile);
                assert_eq!(f.get(x, y), want, "density {density} at ({x}, {y})");
            }
        }
    }
}

#[test]
fn line_pattern_inks_every_other_row() {
    let f =
        draw("g:pattern(48, 48, 64, 64, {kind = 'line', density = 50, color = 15})").raw_frame();
    for y in 48..112 {
        assert_eq!(
            black_in(&f, 48, y, 64, 1),
            if y % 2 == 0 { 64 } else { 0 },
            "row {y}"
        );
    }
}

#[test]
fn cross_pattern_is_a_grid_of_about_57_percent() {
    let f =
        draw("g:pattern(48, 48, 64, 64, {kind = 'cross', density = 50, color = 15})").raw_frame();
    let share = black_in(&f, 48, 48, 64, 64) as f64 / (64.0 * 64.0);
    assert!((share - 0.57).abs() < 0.01, "{share}");
    for (x, y) in [(48, 49), (49, 48), (51, 50)] {
        assert_eq!(f.get(x, y), 1, "({x}, {y})");
    }
    assert_eq!(f.get(49, 49), 0);
}

#[test]
fn white_dots_are_drawn_with_color_0_and_leave_the_rest_alone() {
    let f = draw(
        "g:rect(48, 48, 64, 64, {fill = true})\n\
         g:pattern(48, 48, 64, 64, {kind = 'dot', density = 50, color = 0})",
    )
    .raw_frame();
    assert_eq!(black_in(&f, 48, 48, 64, 64), 64 * 64 / 2);
    for (x, y) in [(48u32, 48u32), (49, 48), (48, 49)] {
        let white = BAYER[(y % 8) as usize][(x % 8) as usize] < 32;
        assert_eq!(f.get(x, y), u8::from(!white), "({x}, {y})");
    }
}

#[test]
fn an_unknown_kind_silently_draws_nothing() {
    let quiet = draw("").log().len();
    for opts in [
        "{kind = 'hatch', density = 50, color = 15}",
        "{kind = 'checker'}",
    ] {
        let mut core = draw(&format!("r = {{g:pattern(48, 48, 64, 64, {opts})}}"));
        assert_eq!(black_in(&core.raw_frame(), 0, 0, 480, 800), 0, "{opts}");
        assert!(
            core.warnings().is_empty(),
            "{opts}: {:?}",
            &*core.warnings()
        );
        assert_eq!(core.log().len(), quiet, "{opts}: {:?}", &*core.log());
        assert_eq!(
            (core.frame_commands(), core.lua_eval("return r[2]").unwrap()),
            (0, serde_json::json!("invalid_argument")),
            "{opts}: API 2.33.1 contract §6 says otherwise: refused, not drawn (06/pattern_invalid_args)"
        );
    }
}

#[test]
fn a_pattern_without_kind_is_a_dot_pattern() {
    for opts in ["{density = 50}", "nil"] {
        let f = draw(&format!("g:pattern(48, 48, 64, 64, {opts})")).raw_frame();
        assert_eq!(
            black_in(&f, 48, 48, 64, 64),
            64 * 64 / 2,
            "{opts}: API 2.33.1 contract §6 says otherwise: kind defaults to dot"
        );
    }
}

#[test]
fn pattern_checks_its_arguments_as_the_other_g_calls() {
    let manifest = r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.2"}"#;
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        (
            "index.lua".to_string(),
            b"function on_draw(c, g) g:pattern(1, 2, 3) end\n".to_vec(),
        ),
    ]));
    let core = Core::new(files, Config::default(), Persisted::default());
    let errors = core.errors();
    assert!(
        errors[0].contains("bad argument #4 to 'pattern' (number expected, got no value)"),
        "{errors:?}"
    );
}

#[test]
fn rect_radius_rounds_the_corners() {
    let f = draw("g:rect(48, 48, 64, 48, {fill = true, radius = 12})").raw_frame();
    for (x, y) in [(48, 48), (111, 48), (48, 95), (111, 95), (50, 50)] {
        assert_eq!(f.get(x, y), 0, "corner ({x}, {y})");
    }
    for (x, y) in [(80, 48), (48, 72), (80, 72), (111, 72), (80, 95)] {
        assert_eq!(f.get(x, y), 1, "edge or centre ({x}, {y})");
    }
    let outline = draw("g:rect(48, 48, 64, 48, {radius = 12})").raw_frame();
    assert_eq!(
        [
            outline.get(48, 48),
            outline.get(80, 48),
            outline.get(80, 72)
        ],
        [0, 1, 0]
    );
}

/// An app whose `on_draw` runs `body`, with `gray` false on the first frame; `layer` is a black 4×4 layer, `img`
/// the package image `data/img.png` (4 pixels: 0, 85, 170, 255). [`next`] draws the next frame.
fn gray_app(body: &str) -> Core {
    let manifest = r#"{"app_id": "testapp", "entry": "index.lua", "api": "2.11"}"#;
    let index = format!(
        "gray = false\n\
         function on_load(c)\n\
           layer = c.layers:create(4, 4)\n\
           layer:draw(function(g) g:rect(0, 0, 4, 4, {{fill = true}}) end)\n\
           img = c.package:image('img.png')\n\
         end\n\
         function on_input(c, ev) if ev.phase == 'up' then c:invalidate() end return true end\n\
         function on_draw(c, g) {body} end\n"
    );
    let mut png = vec![];
    image::GrayImage::from_raw(4, 1, vec![0, 85, 170, 255])
        .unwrap()
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let files = AppFiles(BTreeMap::from([
        ("manifest.json".to_string(), manifest.as_bytes().to_vec()),
        ("index.lua".to_string(), index.into_bytes()),
        ("data/img.png".to_string(), png),
    ]));
    let core = Core::new(files, Config::default(), Persisted::default());
    assert!(core.errors().is_empty(), "{:?}", &*core.errors());
    core
}

/// Draws the next frame with `gray` set to `gray`.
fn next(core: &mut Core, gray: bool) {
    core.lua_exec(&format!("gray = {gray}")).unwrap();
    core.tap(300, 300).unwrap();
}

fn steps(f: &Frame, pts: &[(u32, u32)]) -> Vec<u8> {
    pts.iter().map(|&(x, y)| f.get(x, y)).collect()
}

#[test]
fn every_colour_of_a_gray_frame_is_scaled_to_four_steps() {
    let mut core = gray_app(
        "g:clear(0)\n\
         for i, c in ipairs({3, 4, 8, 12, 15}) do g:rect(i * 10, 0, 10, 10, {fill = true, color = c}) end\n\
         g:line(0, 20, 100, 20, {color = 8})\n\
         g:pattern(0, 30, 8, 8, {density = 100, color = 4})\n\
         g:image(layer, 0, 50, {gray = gray})",
    );
    let normal = core.raw_frame();
    assert_eq!(
        core.frame_gray(),
        None,
        "a frame without gray = true is 1-bit"
    );
    next(&mut core, true);
    let g = core
        .frame_gray()
        .expect("gray = true makes the frame gray (§3)");
    let rects = [(15, 5), (25, 5), (35, 5), (45, 5), (55, 5)];
    assert_eq!(steps(&g, &rects), [0, 1, 2, 3, 3], "(color * 4) // 16 (§3)");
    assert_eq!(steps(&g, &[(50, 20), (3, 33), (1, 51)]), [2, 1, 3]);
    assert_eq!(steps(&g, &[(200, 200)]), [0]);
    assert_eq!(
        core.raw_frame(),
        g.threshold(),
        "the 1-bit frame of a gray frame is its steps' threshold"
    );
    assert_eq!(
        normal,
        g.threshold(),
        "the threshold is what a normal frame draws"
    );
    assert_eq!(
        core.frame(true).px,
        g.threshold().px,
        "no plate is up on a gray frame"
    );
}

#[test]
fn gray_frame_text_takes_its_colour_in_steps() {
    let mut core = gray_app(
        "g:text(10, 10, 'MMMM', {font = 'large', color = 9})\n\
         g:image(layer, 0, 300, {gray = gray})",
    );
    next(&mut core, true);
    let g = core.frame_gray().unwrap();
    let text: Vec<u8> = g.px[..g.w as usize * 100]
        .iter()
        .copied()
        .filter(|&v| v != 0)
        .collect();
    assert!(
        !text.is_empty() && text.iter().all(|&v| v == 2),
        "colour 9 is 2 steps"
    );
}

#[test]
fn gray_frame_images_keep_their_levels_and_tint() {
    let mut core = gray_app(
        "g:image(img, 0, 0, {gray = gray})\n\
         g:image(img, 0, 10, {invert = true})\n\
         g:image(layer, 10, 0, {color = 8})\n\
         g:image(layer, 20, 0, {color = 8, invert = true})",
    );
    next(&mut core, true);
    let g = core.frame_gray().unwrap();
    let row = |y: u32| steps(&g, &[(0, y), (1, y), (2, y), (3, y)]);
    assert_eq!(
        row(0),
        [3, 2, 1, 0],
        "a grayscale image uses its own colour (§6)"
    );
    assert_eq!(row(10), [0, 1, 2, 3], "invert takes 15 - colour (§6)");
    assert_eq!(
        steps(&g, &[(11, 1), (21, 1)]),
        [2, 1],
        "an ink dot of a black-and-white image is `color`; inverted 15 - 8"
    );
}

#[test]
fn a_gray_frame_is_a_gray_refresh_and_the_next_normal_frame_full() {
    let mut core = gray_app("g:image(layer, 0, 0, {gray = gray})");
    for gray in [true, true, false, false] {
        next(&mut core, gray);
    }
    assert_eq!(
        core.status().refreshes,
        ["full", "gray", "gray", "full", "partial"],
        "§3: a gray frame is a full refresh; the next normal frame is full"
    );
    assert_eq!(core.frame_gray(), None);
}

#[test]
fn a_host_screen_over_the_app_makes_the_frame_normal() {
    let mut core = gray_app(
        "if gray then c.dialog:toast('saved') end\n\
         g:image(layer, 0, 0, {gray = gray})",
    );
    next(&mut core, true);
    assert_eq!(
        core.status().refresh,
        Some("full"),
        "§3: a dialog or similar screen over the app turns the frame into a normal full refresh"
    );
    assert_eq!(core.frame_gray(), None);
    core.wait(10_000).unwrap();
    assert_eq!(
        core.frame_gray(),
        None,
        "the panel still shows the normal frame"
    );
}

#[test]
fn a_host_screen_raised_later_shows_the_gray_frame_in_1_bit_while_it_is_up() {
    let mut core = gray_app("g:image(layer, 0, 0, {gray = gray})");
    next(&mut core, true);
    core.lua_exec("ctx.dialog:toast('saved')").unwrap();
    assert_eq!(core.frame_gray(), None);
    core.wait(10_000).unwrap();
    assert!(
        core.frame_gray().is_some(),
        "the toast is gone, no new frame"
    );
    assert_eq!(core.status().refreshes, ["full", "gray"]);
}

#[test]
fn g_clear_takes_its_colour_step_in_a_gray_frame() {
    for (c, want) in [(0, 0), (3, 0), (4, 1), (8, 2), (15, 3)] {
        let mut core = gray_app(&format!(
            "g:clear({c})\n\
             g:image(layer, 0, 0, {{gray = gray}})"
        ));
        let normal = core.raw_frame().get(200, 200);
        assert_eq!(normal, u8::from(c != 0), "a normal frame keeps 0.1's clear");
        next(&mut core, true);
        let g = core.frame_gray().unwrap();
        assert_eq!(
            g.get(200, 200),
            want,
            "clear({c}): the whole gray frame is drawn in steps (§3)"
        );
    }
}

#[test]
fn raw_frame_of_a_gray_frame_is_the_steps_threshold_even_when_covered() {
    // scaled up, the 85 block dithers to a mix in 1 bit but is 2 steps (ink) in the threshold
    let body = "if gray and toast then c.dialog:toast('saved') end\n\
                g:image(img, 0, 0, {gray = gray, w = 40, h = 4})";
    let mut core = gray_app(body);
    let dithered = core.raw_frame();
    next(&mut core, true);
    let steps = core.frame_gray().unwrap().threshold();
    assert!(steps != dithered, "the normal frame dithers the image");
    assert!(core.raw_frame() == steps);
    core.lua_exec("ctx.dialog:toast('later')").unwrap();
    assert_eq!(core.frame_gray(), None);
    assert!(
        core.raw_frame() == steps,
        "a host screen raised later does not change it"
    );
    core.lua_exec("toast = true").unwrap();
    next(&mut core, true);
    assert_eq!(core.status().refresh, Some("full"));
    assert!(
        core.raw_frame() == steps,
        "nor does one up when the frame is presented"
    );
}
