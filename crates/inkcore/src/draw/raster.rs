//! Shapes on a 1-bit frame by the pixel rules of §6; polygons and thick lines as firmware 26112001 draws them.
//! Every loop runs over the shape's box clipped to the frame, so huge coordinates cost nothing.
use super::Frame;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// The pixel value: 1 black, 0 white (a gray frame: ink steps). A `Cmd` keeps the colour 0..15 here.
    pub ink: u8,
    pub fill: bool,
    pub width: i32,
}

impl Frame {
    /// Sets the pixels whose centres satisfy `inside`, within the box `x0..x1`, `y0..y1`.
    fn paint(
        &mut self,
        (x0, y0, x1, y1): (f64, f64, f64, f64),
        ink: u8,
        inside: impl Fn(f64, f64) -> bool,
    ) {
        let clip = |v: f64, max: u32| v.clamp(0.0, f64::from(max)) as u32;
        for py in clip(y0.floor(), self.h)..clip(y1.ceil(), self.h) {
            for px in clip(x0.floor(), self.w)..clip(x1.ceil(), self.w) {
                if inside(f64::from(px) + 0.5, f64::from(py) + 0.5) {
                    self.px[(py * self.w + px) as usize] = ink;
                }
            }
        }
    }

    pub fn clear(&mut self, ink: u8) {
        self.px.fill(ink);
    }

    /// `x`, `y`, `w`, `h` as 0.1 `g:rect`; a radius rounds the corners.
    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, radius: f64, st: Style) {
        // Pillow's box is inclusive: [x, x + w - 1] covers w pixels
        let (l, t, r, b) = (
            x.round(),
            y.round(),
            (x + w - 1.0).round() + 1.0,
            (y + h - 1.0).round() + 1.0,
        );
        let rad = radius.max(0.0).min((r - l) / 2.0).min((b - t) / 2.0);
        let lw = f64::from(st.width);
        let inside = move |px: f64, py: f64, d: f64| {
            in_round_rect(px, py, l + d, t + d, r - d, b - d, (rad - d).max(0.0))
        };
        self.paint((l, t, r, b), st.ink, |px, py| {
            inside(px, py, 0.0) && (st.fill || !inside(px, py, lw))
        });
    }

    pub fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, width: i32, ink: u8) {
        let (x0, y0, x1, y1) = (x0.round(), y0.round(), x1.round(), y1.round());
        if width <= 1 {
            self.thin_line(x0, y0, x1, y1, ink);
            return;
        }
        let half = f64::from(width) / 2.0;
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = dx * dx + dy * dy;
        let bbox = (
            x0.min(x1) - half,
            y0.min(y1) - half,
            x0.max(x1) + half + 1.0,
            y0.max(y1) + half + 1.0,
        );
        // the ends are pixel centres; firmware 26112001 rounds the caps (distance to the segment, not its line)
        let (ax, ay) = (x0 + 0.5, y0 + 0.5);
        self.paint(bbox, ink, |px, py| {
            let t = if len2 == 0.0 {
                0.0
            } else {
                (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
            };
            (px - ax - t * dx).hypot(py - ay - t * dy) < half
        });
    }

    /// One pixel per step along the major axis, over the part inside the frame only.
    fn thin_line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, ink: u8) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let (w, h) = (f64::from(self.w), f64::from(self.h));
        if dx.abs() >= dy.abs() {
            let (from, to) = (x0.min(x1).max(0.0), x0.max(x1).min(w - 1.0));
            let mut x = from;
            while x <= to {
                let y = if dx == 0.0 {
                    y0
                } else {
                    (y0 + (x - x0) * dy / dx).round()
                };
                self.set_f(x, y, ink);
                x += 1.0;
            }
        } else {
            let (from, to) = (y0.min(y1).max(0.0), y0.max(y1).min(h - 1.0));
            let mut y = from;
            while y <= to {
                self.set_f((x0 + (y - y0) * dx / dy).round(), y, ink);
                y += 1.0;
            }
        }
    }

    fn set_f(&mut self, x: f64, y: f64, ink: u8) {
        if x >= 0.0 && y >= 0.0 && x < f64::from(self.w) && y < f64::from(self.h) {
            self.px[(y as u32 * self.w + x as u32) as usize] = ink;
        }
    }

    pub fn polyline(&mut self, pts: &[(f64, f64)], width: i32, ink: u8) {
        for seg in pts.windows(2) {
            self.line(seg[0].0, seg[0].1, seg[1].0, seg[1].1, width, ink);
        }
    }

    /// Circle of radius `r` around the centre of pixel `x`, `y` (§6): fill covers the pixel centres closer than
    /// `r`, a stroke those of them at `r - width` or more. With `arc`, only the stroke from `a0` clockwise to `a1`
    /// degrees (0 is 3 o'clock); a span of whole turns draws nothing.
    pub fn circle(&mut self, x: f64, y: f64, r: f64, arc: Option<(f64, f64)>, st: Style) {
        let span = arc.map(|(a0, a1)| (a0, (a1 - a0).rem_euclid(360.0)));
        if span.is_some_and(|(_, s)| s == 0.0) {
            return;
        }
        let (cx, cy) = (x + 0.5, y + 0.5);
        let inner = if st.fill && arc.is_none() {
            0.0
        } else {
            (r - f64::from(st.width)).max(0.0)
        };
        let in_angle = |px: f64, py: f64| match span {
            None => true,
            Some((a0, s)) => ((py - cy).atan2(px - cx).to_degrees() - a0).rem_euclid(360.0) <= s,
        };
        self.paint((cx - r, cy - r, cx + r, cy + r), st.ink, |px, py| {
            let d2 = (px - cx).powi(2) + (py - cy).powi(2);
            d2 < r * r && d2 >= inner * inner && in_angle(px, py)
        });
    }

    /// A vertex `(x, y)` is the centre of pixel `(x, y)`, as for a line end (firmware 26112001): fill covers the
    /// pixels whose centre is inside by the even-odd rule, with no outline; a stroke is the closed polyline.
    pub fn polygon(&mut self, pts: &[(f64, f64)], st: Style) {
        if st.fill {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for &(x, y) in pts {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1.0), y1.max(y + 1.0));
            }
            self.paint((x0, y0, x1, y1), st.ink, |px, py| {
                even_odd(pts, px - 0.5, py - 0.5)
            });
            return;
        }
        let closed = pts.iter().chain(pts.first());
        for (a, b) in closed.clone().zip(closed.skip(1)) {
            self.line(a.0, a.1, b.0, b.1, st.width, st.ink);
        }
    }
}

/// Whether the point `px`, `py` of a `w`×`h` box at the origin lies inside its corners rounded by `radius`, as
/// `g:rect{radius}` rounds them (a radius over half the short side is half the short side).
pub(crate) fn in_rounded(px: f64, py: f64, w: f64, h: f64, radius: f64) -> bool {
    let rad = radius.max(0.0).min(w / 2.0).min(h / 2.0);
    in_round_rect(px, py, 0.0, 0.0, w, h, rad)
}

fn in_round_rect(px: f64, py: f64, l: f64, t: f64, r: f64, b: f64, rad: f64) -> bool {
    if px < l || px > r || py < t || py > b {
        return false;
    }
    let (qx, qy) = (px.clamp(l + rad, r - rad), py.clamp(t + rad, b - rad));
    (px - qx).powi(2) + (py - qy).powi(2) <= rad * rad
}

fn even_odd(pts: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let ((xi, yi), (xj, yj)) = (pts[i], pts[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}
