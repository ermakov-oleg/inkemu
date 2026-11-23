//! 1-bit text: the bundled DejaVu Sans (0.1 `fonts.py`), or the user's BDF files drawn 1:1; widths, line
//! breaking and the text styles of API 2.33 §7.
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

/// `small`, `medium` and `large` on x4_pro (§7); the bundled font is drawn on these baselines.
const ASCENTS: [i32; 3] = [17, 21, 26];

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

/// The 2.16 and 2.28 text options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStyle {
    pub bold: bool,
    pub tabular: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

impl TextStyle {
    /// Underline and strikethrough lines under one line of text: each is one more draw command.
    pub fn strokes(&self) -> usize {
        usize::from(self.underline) + usize::from(self.strike)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wrap {
    #[default]
    Word,
    Char,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    #[default]
    Clip,
    Ellipsis,
    Visible,
}

/// What `g:text_box` and `font:layout` read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutOpts {
    pub wrap: Wrap,
    pub overflow: Overflow,
    pub line_gap: i32,
    pub style: TextStyle,
    /// The 2.16 line-start and line-end rules.
    pub kinsoku: bool,
}

/// How a line is drawn: 1 black or 0 white, its style, and the columns `clip.0..clip.1` it may touch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Paint {
    pub ink: u8,
    pub style: TextStyle,
    pub clip: Option<(i32, i32)>,
}

/// One laid-out line: what is drawn and its `measure` width (bold included).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub width: i32,
}

/// `font:layout`: the lines that fit, their block height and the bytes they take from the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub lines: Vec<Line>,
    pub height: i32,
    pub consumed: usize,
}

/// The three device fonts: DejaVu Sans by default, or BDF fonts in `FONT_NAMES` order.
#[derive(Clone, Debug, Default)]
pub struct Fonts {
    bdf: Option<Rc<[BdfFont; 3]>>,
}

/// The advances of one font and style: `measure`, the line breaker and the pen all use these.
struct Pen<'a> {
    bdf: Option<&'a BdfFont>,
    px: f32,
    /// The tabular digit cell: the widest digit.
    cell: Option<i32>,
}

impl Pen<'_> {
    fn glyph_adv(&self, c: char) -> i32 {
        match self.bdf {
            Some(b) => b.glyph(c).map_or(0, |g| g.advance),
            None => FONT.metrics(c, self.px).advance_width.round() as i32,
        }
    }

    fn adv(&self, c: char) -> i32 {
        match c {
            '\t' => self.glyph_adv(' '),
            c if c.is_control() => 0,
            c if c.is_ascii_digit() => self.cell.unwrap_or_else(|| self.glyph_adv(c)),
            c => self.glyph_adv(c),
        }
    }

    fn width(&self, s: &str) -> i64 {
        s.chars().map(|c| i64::from(self.adv(c))).sum()
    }
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

    fn pen(&self, font: &str, style: TextStyle) -> Pen<'_> {
        let mut pen = Pen {
            bdf: self.bdf(font),
            px: self.px(font),
            cell: None,
        };
        if style.tabular {
            pen.cell = ('0'..='9').map(|d| pen.glyph_adv(d)).max();
        }
        pen
    }

    /// The width of one line: the sum of the advances (no kerning), which is also the drawn width.
    pub fn measure(&self, font: &str, s: &str) -> i32 {
        self.measure_with(font, s, TextStyle::default())
    }

    /// `font:measure(s, opts)`: bold adds one pixel to a line that has any width.
    pub fn measure_with(&self, font: &str, s: &str, style: TextStyle) -> i32 {
        let w = self.pen(font, style).width(s);
        clamp(w + i64::from(style.bold && w > 0))
    }

    pub fn ascent(&self, font: &str) -> i32 {
        match self.bdf(font) {
            Some(b) => b.ascent,
            None => ASCENTS[tier(font)],
        }
    }

    /// Whether the font has its own glyph for `c` (not the missing-glyph one).
    pub fn has_glyph(&self, font: &str, c: char) -> bool {
        match self.bdf(font) {
            Some(b) => b.has(c),
            None => FONT.lookup_glyph_index(c) != 0,
        }
    }

    /// Draws `s` in `ink` (1 black, 0 white) in a line box of the font's line height starting at `top`.
    pub fn draw(&self, frame: &mut Frame, font: &str, x: i32, top: i32, s: &str, ink: u8) {
        let paint = Paint {
            ink,
            ..Paint::default()
        };
        self.draw_styled(frame, font, (x, top), s, paint);
    }

    /// Draws one line of text in its line box at `x`, `top` (without the strokes, see `draw_strokes`).
    pub fn draw_styled(
        &self,
        frame: &mut Frame,
        font: &str,
        (x, top): (i32, i32),
        s: &str,
        p: Paint,
    ) {
        let (style, ink, clip) = (p.style, p.ink, p.clip);
        let pen = self.pen(font, style);
        // i64: a far-off text position from the app must not overflow
        let base = i64::from(top) + i64::from(self.ascent(font));
        let mut at = i64::from(x);
        let mut cells = vec![];
        for c in s.chars() {
            let adv = pen.adv(c);
            if !c.is_control() {
                // a tabular digit is centred in its cell, rounding toward the left
                let off = (adv - pen.glyph_adv(c)).div_euclid(2);
                self.glyph(&pen, c, at + i64::from(off), base, &mut cells);
            }
            at += i64::from(adv);
        }
        for (gx, gy, bottom) in cells {
            // italic: about 18% to the right per row above the glyph's bottom row
            let shear = if style.italic {
                (bottom - gy) * 18 / 100
            } else {
                0
            };
            for dx in 0..=i64::from(style.bold) {
                put(frame, gx + shear + dx, gy, ink, clip);
            }
        }
    }

    /// The ink pixels of `c` with its pen at `pen_x`, each with the glyph's bottom row.
    fn glyph(&self, pen: &Pen, c: char, pen_x: i64, base: i64, out: &mut Vec<(i64, i64, i64)>) {
        if let Some(b) = pen.bdf {
            let Some(g) = b.glyph(c) else { return };
            let (gx, gy) = (
                pen_x + i64::from(g.xoff),
                base - i64::from(g.yoff) - i64::from(g.h),
            );
            let bottom = gy + i64::from(g.h) - 1;
            for row in 0..g.h {
                for col in (0..g.w).filter(|&col| g.ink(col, row)) {
                    out.push((gx + i64::from(col), gy + i64::from(row), bottom));
                }
            }
            return;
        }
        // a character the font lacks draws nothing but advances
        let index = FONT.lookup_glyph_index(c);
        if index == 0 {
            return;
        }
        let (m, bitmap) = FONT.rasterize_indexed(index, pen.px);
        let gx = pen_x + i64::from(m.xmin);
        let gy = base - i64::from(m.ymin) - m.height as i64;
        let bottom = gy + m.height as i64 - 1;
        for (i, &cov) in bitmap.iter().enumerate() {
            if cov >= 128 {
                out.push((gx + (i % m.width) as i64, gy + (i / m.width) as i64, bottom));
            }
        }
    }

    /// The 2.28 underline and strikethrough of a line of `width` pixels whose line box starts at `x`, `top`.
    pub fn draw_strokes(
        &self,
        frame: &mut Frame,
        font: &str,
        (x, top, width): (i32, i32, i32),
        p: Paint,
    ) {
        let (style, ink, clip) = (p.style, p.ink, p.clip);
        if width <= 0 {
            return;
        }
        let (lh, ascent) = (self.line_height(font), self.ascent(font));
        let t = ((lh + 12) / 24).max(1);
        let mut tops = vec![];
        if style.underline {
            tops.push((ascent + 1).min(lh - t));
        }
        if style.strike {
            tops.push(lh / 2 - t / 2);
        }
        for dy in tops {
            for row in 0..t {
                let y = i64::from(top) + i64::from(dy + row);
                for col in 0..width {
                    put(frame, i64::from(x) + i64::from(col), y, ink, clip);
                }
            }
        }
    }

    /// Word-wrapped lines of at most `width` pixels, every line kept: the host's own plates.
    pub fn wrap(&self, font: &str, s: &str, width: f64) -> Vec<String> {
        let o = LayoutOpts {
            overflow: Overflow::Visible,
            ..LayoutOpts::default()
        };
        let lay = self.layout(font, s, width, 0.0, &o);
        lay.lines.into_iter().map(|l| l.text).collect()
    }

    /// The lines `g:text_box` draws in a `w` by `h` box, as `font:layout` counts them.
    pub fn layout(&self, font: &str, s: &str, w: f64, h: f64, o: &LayoutOpts) -> Layout {
        let pen = self.pen(font, o.style);
        let (lh, gap) = (self.line_height(font), o.line_gap);
        let max_lines = match o.overflow {
            Overflow::Visible => usize::MAX,
            _ if h.is_nan() || h < f64::from(lh) => 0,
            _ => (((h - f64::from(lh)) / f64::from(lh + gap)).floor() as usize).saturating_add(1),
        };
        // a bold line is one pixel wider, so it wraps at w - 1
        let avail = w - f64::from(u8::from(o.style.bold));
        let mut out = Layout::default();
        // each line's text and the byte it starts at
        let mut lines: Vec<(&str, usize)> = vec![];
        let mut start = 0;
        'paras: while start < s.len() {
            let end = s[start..].find('\n').map_or(s.len(), |i| start + i);
            let after = if end < s.len() { end + 1 } else { end };
            let para: Vec<(usize, char, i64)> = s[start..end]
                .char_indices()
                .map(|(i, c)| (start + i, c, i64::from(pen.adv(c))))
                .collect();
            let mut i = 0;
            loop {
                if lines.len() >= max_lines {
                    break 'paras;
                }
                let j = match o.wrap {
                    Wrap::None => para.len(),
                    _ => next_break(&para, i, avail, o),
                };
                let (a, b) = trim(&para, i, j);
                lines.push(match (para.get(a), para.get(b.wrapping_sub(1))) {
                    (Some(first), Some(last)) if a < b => {
                        (&s[first.0..last.0 + last.1.len_utf8()], first.0)
                    }
                    _ => ("", start),
                });
                out.consumed = para.get(j).map_or(after, |p| p.0);
                i = j;
                if i >= para.len() {
                    break;
                }
            }
            start = after;
        }
        let more = out.consumed < s.len();
        let n = lines.len();
        for (k, (text, at)) in lines.into_iter().enumerate() {
            let mut line = text.to_string();
            if o.overflow == Overflow::Ellipsis
                && (pen.width(text) as f64 > avail || (more && k + 1 == n))
            {
                let kept;
                (line, kept) = self.shorten(font, &pen, text, avail);
                if more && k + 1 == n && kept < text.len() {
                    out.consumed = at + kept;
                }
            }
            out.lines.push(Line {
                width: self.measure_with(font, &line, o.style),
                text: line,
            });
        }
        let n = n as i32;
        if n > 0 {
            out.height = n
                .saturating_mul(lh)
                .saturating_add((n - 1).saturating_mul(gap));
        }
        out
    }

    /// `text` ended with an ellipsis, characters removed from its end until both fit in `avail`; and the bytes
    /// of `text` kept.
    fn shorten(&self, font: &str, pen: &Pen, text: &str, avail: f64) -> (String, usize) {
        let ellipsis = if self.has_glyph(font, '…') {
            "…"
        } else {
            "..."
        };
        let ell = pen.width(ellipsis);
        let chars: Vec<char> = text.trim_end_matches(is_space).chars().collect();
        let mut sums = vec![0i64];
        for &c in &chars {
            sums.push(sums[sums.len() - 1] + i64::from(pen.adv(c)));
        }
        let mut n = chars.len();
        while n > 0 && (sums[n] + ell) as f64 > avail {
            // one whole character: its marks, then the base, then a Thai vowel written before it
            while n > 0 {
                n -= 1;
                if !joins_prev(chars[n]) {
                    break;
                }
            }
            while n > 0 && (joins_next(chars[n - 1]) || is_space(chars[n - 1])) {
                n -= 1;
            }
        }
        let kept: String = chars[..n].iter().collect();
        let n = kept.len();
        (kept + ellipsis, n)
    }
}

fn clamp(w: i64) -> i32 {
    w.clamp(0, i64::from(i32::MAX)) as i32
}

fn put(frame: &mut Frame, x: i64, y: i64, ink: u8, clip: Option<(i32, i32)>) {
    if clip.is_some_and(|(a, b)| x < i64::from(a) || x >= i64::from(b)) {
        return;
    }
    if let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) {
        frame.set(x, y, ink);
    }
}

/// Whitespace that wraps and takes no width at the ends of a line; no-break spaces stay glyphs.
fn is_space(c: char) -> bool {
    c.is_whitespace() && !matches!(c, '\u{a0}' | '\u{2007}' | '\u{202f}')
}

/// The content of `para[i..j]`: without whitespace at either end.
fn trim(para: &[(usize, char, i64)], i: usize, j: usize) -> (usize, usize) {
    let mut a = i;
    while a < j && is_space(para[a].1) {
        a += 1;
    }
    let mut b = j;
    while b > a && is_space(para[b - 1].1) {
        b -= 1;
    }
    (a, b)
}

/// Where the line starting at `para[i]` ends: the index the next line starts at (`para.len()` when the rest
/// fits). At least one character is taken.
fn next_break(para: &[(usize, char, i64)], i: usize, avail: f64, o: &LayoutOpts) -> usize {
    let n = para.len();
    let mut k = i;
    while k < n && is_space(para[k].1) {
        k += 1;
    }
    let (mut w, mut brk) = (0i64, None);
    let mut j = k;
    while j < n {
        let (_, c, a) = para[j];
        if j > k && !is_space(c) && (w + a) as f64 > avail {
            let p = brk.or_else(|| (k + 1..=j).rev().find(|&q| boundary(para, q)));
            // the first character alone is wider than the box: it still makes a line
            let p = p.unwrap_or_else(|| {
                (k + 1..=n)
                    .find(|&q| q == n || boundary(para, q))
                    .unwrap_or(n)
            });
            return if o.kinsoku { kinsoku(para, k, p) } else { p };
        }
        w += a;
        j += 1;
        if j < n && breaks(para, j, o.wrap) {
            brk = Some(j);
        }
    }
    n
}

/// Whether a line may break before `para[j]`.
fn breaks(para: &[(usize, char, i64)], j: usize, wrap: Wrap) -> bool {
    let (a, b) = (para[j - 1].1, para[j].1);
    if is_space(b) || !boundary(para, j) {
        return false;
    }
    match wrap {
        Wrap::Char => true,
        Wrap::Word => is_space(a) || wide(a) || wide(b) || a == '\u{0f0b}',
        Wrap::None => false,
    }
}

/// Whether `para[j]` starts a new character: combining marks and Thai vowels stay with their base.
fn boundary(para: &[(usize, char, i64)], j: usize) -> bool {
    j == 0 || j >= para.len() || !(joins_prev(para[j].1) || joins_next(para[j - 1].1))
}

/// 2.16: a break before a character that may not start a line, or after one that may not end it, moves back
/// up to 8 code points to a break that breaks neither rule; else it stays.
fn kinsoku(para: &[(usize, char, i64)], k: usize, p: usize) -> usize {
    let bad = |q: usize| {
        q < para.len() && (NO_START.contains(&para[q].1) || NO_END.contains(&para[q - 1].1))
    };
    if !bad(p) {
        return p;
    }
    let mut q = p;
    for _ in 0..8 {
        q -= 1;
        if q <= k {
            break;
        }
        // en-07: the moved break must be a valid `word` break point, in `char` wrapping too
        if !bad(q) && breaks(para, q, Wrap::Word) {
            return q;
        }
    }
    p
}

/// CJK and Thai: a line may break beside them.
fn wide(c: char) -> bool {
    matches!(c as u32,
        0x0E00..=0x0E7F | 0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F
        | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
}

/// Combining marks, Thai and Tibetan marks and the Thai following vowels: never the start of a line.
fn joins_prev(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF
        | 0x200C..=0x200D | 0x20D0..=0x20FF | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F
        | 0x0E30..=0x0E3A | 0x0E45..=0x0E4E
        | 0x0F18..=0x0F19 | 0x0F35 | 0x0F37 | 0x0F39 | 0x0F3E..=0x0F3F | 0x0F71..=0x0F84
        | 0x0F86..=0x0F87 | 0x0F8D..=0x0FBC | 0x0FC6)
}

/// The Thai leading vowels: never the end of a line.
fn joins_next(c: char) -> bool {
    matches!(c as u32, 0x0E40..=0x0E44)
}

/// 2.16: may not start a line.
const NO_START: &[char] = &[
    ',', '.', ':', ';', '!', '?', ')', ']', '}', '\u{3001}', '\u{3002}', '\u{FF0C}', '\u{FF0E}',
    '\u{FF1A}', '\u{FF1B}', '\u{FF01}', '\u{FF1F}', '\u{300D}', '\u{300F}', '\u{201D}', '\u{2019}',
    '\u{FF09}', '\u{3010}', '\u{3011}', '\u{300B}', '\u{3009}', '\u{3015}', '\u{FF3D}', '\u{FF5D}',
    '\u{3017}', '\u{3019}', '\u{301B}', '\u{3041}', '\u{3043}', '\u{3045}', '\u{3047}', '\u{3049}',
    '\u{3063}', '\u{3083}', '\u{3085}', '\u{3087}', '\u{308E}', '\u{3095}', '\u{3096}', '\u{30A1}',
    '\u{30A3}', '\u{30A5}', '\u{30A7}', '\u{30A9}', '\u{30C3}', '\u{30E3}', '\u{30E5}', '\u{30E7}',
    '\u{30EE}', '\u{30F5}', '\u{30F6}', '\u{30FC}', '\u{3005}', '\u{309D}', '\u{309E}', '\u{30FD}',
    '\u{30FE}', '\u{060C}', '\u{061B}', '\u{061F}', '\u{06D4}', '\u{066A}', '\u{066B}', '\u{066C}',
];

/// 2.16: may not end a line.
const NO_END: &[char] = &[
    '(', '[', '{', '"', '\u{2018}', '\u{201C}', '\u{300C}', '\u{300E}', '\u{FF08}', '\u{3010}',
    '\u{300A}', '\u{3008}', '\u{3014}', '\u{3016}', '\u{3018}', '\u{301A}', '\u{FF3B}', '\u{FF5B}',
];
