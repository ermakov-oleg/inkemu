//! Patterns anchored to the screen, not to the rectangle, so neighbouring blocks join without a seam.
use super::Frame;

/// The kinds `g:pattern` draws on firmware 26112001; the device refuses any other with `invalid_argument`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pattern {
    /// Ordered dither: of every 8×8 tile, this many pixels are inked.
    Dot(u8),
    /// Every other row.
    Line,
    /// A grid with a 3 px pitch.
    Cross,
}

/// The standard 8×8 Bayer matrix; it matched the device's `dot` pattern pixel for pixel at all 7 measured
/// densities, with (0, 0) at the screen origin.
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

impl Pattern {
    /// `density` in percent of ink; the device inked 6 of 64 pixels for 10 and 61 for 95.
    pub fn dot(density: f64) -> Pattern {
        Pattern::Dot((density.clamp(0.0, 100.0) * 64.0 / 100.0).round() as u8)
    }

    fn inks(self, x: i32, y: i32) -> bool {
        match self {
            Pattern::Dot(n) => BAYER[y as usize % 8][x as usize % 8] < n,
            Pattern::Line => y % 2 == 0,
            Pattern::Cross => x % 3 == 0 || y % 3 == 0,
        }
    }
}

/// Visits the pixels of `x`, `y`, `w`, `h` inside the frame.
fn each(
    frame: &mut Frame,
    (x, y, w, h): (i32, i32, i32, i32),
    mut f: impl FnMut(&mut Frame, i32, i32),
) {
    let (x0, y0) = (x.max(0), y.max(0));
    let x1 = x.saturating_add(w).min(frame.w as i32);
    let y1 = y.saturating_add(h).min(frame.h as i32);
    for py in y0..y1 {
        for px in x0..x1 {
            f(frame, px, py);
        }
    }
}

/// `g:pattern`: sets the pattern's pixels to `ink` and leaves the others as they are.
pub fn fill(frame: &mut Frame, rect: (i32, i32, i32, i32), kind: Pattern, ink: u8) {
    each(frame, rect, |f, px, py| {
        if kind.inks(px, py) {
            f.set(px, py, ink);
        }
    });
}
