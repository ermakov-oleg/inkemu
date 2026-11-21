//! The device's 50 % grey: a checkerboard anchored to the screen, not to the rectangle.
use super::Frame;

/// `g:pattern_rect` / `g:fill_pattern` over integer coordinates (0.1 truncates them).
pub fn fill(frame: &mut Frame, x: i32, y: i32, w: i32, h: i32) {
    let (x0, y0) = (x.max(0), y.max(0));
    let x1 = x.saturating_add(w).min(frame.w as i32);
    let y1 = y.saturating_add(h).min(frame.h as i32);
    for py in y0..y1 {
        for px in x0..x1 {
            if (px + py) % 2 == 0 {
                frame.set(px, py, 1);
            }
        }
    }
}
