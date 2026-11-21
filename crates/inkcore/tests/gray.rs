//! Grey on the 1-bit canvas as measured on firmware 26112001 (`research-26112001/gray.md`): `color` is a
//! threshold and `g:pattern` dithers.
use std::collections::BTreeMap;

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
        "{density = 50}",
        "nil",
    ] {
        let core = draw(&format!("g:pattern(48, 48, 64, 64, {opts})"));
        assert_eq!(black_in(&core.raw_frame(), 0, 0, 480, 800), 0, "{opts}");
        assert!(
            core.warnings().is_empty(),
            "{opts}: {:?}",
            &*core.warnings()
        );
        assert_eq!(core.log().len(), quiet, "{opts}: {:?}", &*core.log());
        assert_eq!(core.frame_commands(), 1, "{opts}: still one command");
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
