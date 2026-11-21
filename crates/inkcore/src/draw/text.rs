//! 1-bit text: the bundled DejaVu Sans (0.1 `fonts.py`), or the user's BDF files drawn 1:1.
use std::rc::Rc;
use std::sync::LazyLock;

use fontdue::{Font, FontSettings};

use super::bdf::BdfFont;
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

fn tier(font: &str) -> usize {
    match font_name(font) {
        "small" => 0,
        "large" => 2,
        _ => 1,
    }
}

/// The three device fonts: DejaVu Sans by default, or BDF fonts in `FONT_NAMES` order.
#[derive(Clone, Debug, Default)]
pub struct Fonts {
    bdf: Option<Rc<[BdfFont; 3]>>,
}

impl Fonts {
    pub fn new() -> Self {
        Fonts { bdf: None }
    }

    /// `small.bdf`, `medium.bdf` and `large.bdf`; the error names the file.
    pub fn from_bdf(files: &[Vec<u8>; 3]) -> Result<Self, String> {
        let mut parsed = vec![];
        for (name, bytes) in FONT_NAMES.iter().zip(files) {
            parsed.push(BdfFont::parse(bytes).map_err(|e| format!("{name}.bdf: {e}"))?);
        }
        let parsed: [BdfFont; 3] = parsed.try_into().expect("three fonts");
        Ok(Fonts {
            bdf: Some(Rc::new(parsed)),
        })
    }

    fn bdf(&self, font: &str) -> Option<&BdfFont> {
        self.bdf.as_deref().map(|b| &b[tier(font)])
    }

    pub fn line_height(&self, font: &str) -> i32 {
        match self.bdf(font) {
            Some(b) => b.line_height,
            None => [20, 24, 30][tier(font)],
        }
    }

    fn px(&self, font: &str) -> f32 {
        (self.line_height(font) as f32 * 0.85).floor()
    }

    fn advance(&self, c: char, px: f32) -> f32 {
        FONT.metrics(c, px).advance_width
    }

    /// The advance width, which is also the drawn width; BDF fonts have no kerning.
    pub fn measure(&self, font: &str, s: &str) -> i32 {
        if let Some(b) = self.bdf(font) {
            return s
                .chars()
                .filter_map(|c| b.glyph(c))
                .map(|g| g.advance)
                .sum();
        }
        let px = self.px(font);
        s.chars().map(|c| self.advance(c, px)).sum::<f32>().round() as i32
    }

    pub fn ascent(&self, font: &str) -> i32 {
        if let Some(b) = self.bdf(font) {
            return b.ascent;
        }
        FONT.horizontal_line_metrics(self.px(font))
            .map_or(self.line_height(font), |m| m.ascent.round() as i32)
    }

    /// Draws `s` in `ink` (1 black, 0 white) in a line box of the font's line height starting at `top`.
    pub fn draw(&self, frame: &mut Frame, font: &str, x: i32, top: i32, s: &str, ink: u8) {
        // i64: a far-off text position from the app must not overflow
        let mut set = |x: i64, y: i64| {
            if let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) {
                frame.set(x, y, ink);
            }
        };
        if let Some(b) = self.bdf(font) {
            // the device puts the baseline at top + ascent
            let base = i64::from(top) + i64::from(b.ascent);
            let mut pen = i64::from(x);
            for g in s.chars().filter_map(|c| b.glyph(c)) {
                let (gx, gy) = (
                    pen + i64::from(g.xoff),
                    base - i64::from(g.yoff) - i64::from(g.h),
                );
                for row in 0..g.h {
                    for col in (0..g.w).filter(|&col| g.ink(col, row)) {
                        set(gx + i64::from(col), gy + i64::from(row));
                    }
                }
                pen += i64::from(g.advance);
            }
            return;
        }
        let (px, ascent) = (self.px(font), self.ascent(font));
        let base = i64::from(top) + i64::from((self.line_height(font) - ascent) / 2 + ascent);
        let mut pen = x as f32;
        for c in s.chars() {
            // a character the font lacks draws nothing but advances
            let index = FONT.lookup_glyph_index(c);
            if index == 0 {
                pen += FONT.metrics_indexed(0, px).advance_width;
                continue;
            }
            let (m, bitmap) = FONT.rasterize_indexed(index, px);
            let gx = (pen + m.xmin as f32).round() as i64;
            let gy = base - i64::from(m.ymin) - m.height as i64;
            for (i, &cov) in bitmap.iter().enumerate() {
                if cov >= 128 {
                    set(gx + (i % m.width) as i64, gy + (i / m.width) as i64);
                }
            }
            pen += m.advance_width;
        }
    }
}
