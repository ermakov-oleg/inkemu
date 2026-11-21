//! 1-bit text in the bundled DejaVu Sans (0.1 `fonts.py`).
use std::sync::LazyLock;

use fontdue::{Font, FontSettings};

use super::Frame;

static FONT: LazyLock<Font> = LazyLock::new(|| {
    let bytes = include_bytes!("../../../../assets/fonts/DejaVuSans.ttf");
    Font::from_bytes(&bytes[..], FontSettings::default()).expect("the bundled font is valid")
});

pub const FONT_NAMES: [&str; 3] = ["small", "medium", "large"];

/// A font name the device knows; anything else is medium.
pub fn font_name(name: &str) -> &'static str {
    FONT_NAMES
        .into_iter()
        .find(|&n| n == name)
        .unwrap_or("medium")
}

#[derive(Clone, Copy)]
pub struct Fonts {
    font: &'static Font,
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

impl Fonts {
    pub fn new() -> Self {
        Fonts { font: &FONT }
    }

    pub fn line_height(&self, font: &str) -> i32 {
        match font_name(font) {
            "small" => 20,
            "large" => 30,
            _ => 24,
        }
    }

    fn px(&self, font: &str) -> f32 {
        (self.line_height(font) as f32 * 0.85).floor()
    }

    fn advance(&self, c: char, px: f32) -> f32 {
        self.font.metrics(c, px).advance_width
    }

    /// The advance width, which is also the drawn width.
    pub fn measure(&self, font: &str, s: &str) -> i32 {
        let px = self.px(font);
        s.chars().map(|c| self.advance(c, px)).sum::<f32>().round() as i32
    }

    pub fn ascent(&self, font: &str) -> i32 {
        self.font
            .horizontal_line_metrics(self.px(font))
            .map_or(self.line_height(font), |m| m.ascent.round() as i32)
    }

    /// Draws `s` black in a line box of the font's line height starting at `top`; a character the font lacks
    /// draws nothing but advances.
    pub fn draw(&self, frame: &mut Frame, font: &str, x: i32, top: i32, s: &str) {
        let (px, ascent) = (self.px(font), self.ascent(font));
        // i64: a far-off text position from the app must not overflow
        let base = i64::from(top) + i64::from((self.line_height(font) - ascent) / 2 + ascent);
        let mut pen = x as f32;
        for c in s.chars() {
            let index = self.font.lookup_glyph_index(c);
            if index == 0 {
                pen += self.font.metrics_indexed(0, px).advance_width;
                continue;
            }
            let (m, bitmap) = self.font.rasterize_indexed(index, px);
            let gx = (pen + m.xmin as f32).round() as i64;
            let gy = base - i64::from(m.ymin) - m.height as i64;
            for (i, &cov) in bitmap.iter().enumerate() {
                let (x, y) = (gx + (i % m.width) as i64, gy + (i / m.width) as i64);
                if let (true, Ok(x), Ok(y)) = (cov >= 128, i32::try_from(x), i32::try_from(y)) {
                    frame.set(x, y, 1);
                }
            }
            pen += m.advance_width;
        }
    }
}
