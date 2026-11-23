//! A BDF 2.1 bitmap font read from bytes: the user's copy of the device font (`INKEMU_FONTS`).
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    /// `DWIDTH`: how far the pen moves.
    pub advance: i32,
    pub w: u32,
    pub h: u32,
    /// `BBX` offsets: from the pen to the left edge, and from the baseline up to the bottom row.
    pub xoff: i32,
    pub yoff: i32,
    bits: Vec<bool>,
}

impl Glyph {
    /// Whether the pixel at `col`, `row` (from the top) is ink.
    pub fn ink(&self, col: u32, row: u32) -> bool {
        col < self.w && row < self.h && self.bits[(row * self.w + col) as usize]
    }
}

#[derive(Clone, Debug)]
pub struct BdfFont {
    pub line_height: i32,
    pub ascent: i32,
    glyphs: HashMap<char, Glyph>,
    default: Option<char>,
}

impl BdfFont {
    /// The glyph of `c`, or of `DEFAULT_CHAR` when the font lacks it.
    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        self.glyphs
            .get(&c)
            .or_else(|| self.default.and_then(|d| self.glyphs.get(&d)))
    }

    /// Whether the font has its own glyph for `c`, not the `DEFAULT_CHAR` one.
    pub fn has(&self, c: char) -> bool {
        self.glyphs.contains_key(&c)
    }

    pub fn parse(bytes: &[u8]) -> Result<BdfFont, String> {
        let text = String::from_utf8_lossy(bytes);
        let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l.trim()));
        match lines.next() {
            Some((_, l)) if l.starts_with("STARTFONT") => {}
            _ => return Err("not a BDF font: it does not start with STARTFONT".into()),
        }
        let (mut pixel_size, mut ascent, mut descent, mut default) = (None, None, None, None);
        let mut bbox = None;
        let mut glyphs = HashMap::new();
        while let Some((n, line)) = lines.next() {
            let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
            let bad = |what: &str| format!("line {n}: bad {what}: {line}");
            let int = |s: &str| s.trim().parse::<i32>().map_err(|_| bad(key));
            match key {
                "PIXEL_SIZE" => pixel_size = Some(int(rest)?),
                "FONT_ASCENT" => ascent = Some(int(rest)?),
                "FONT_DESCENT" => descent = Some(int(rest)?),
                "DEFAULT_CHAR" => default = Some(int(rest)?),
                "FONTBOUNDINGBOX" => bbox = Some(ints::<4>(rest).ok_or_else(|| bad(key))?),
                "STARTCHAR" => {
                    if let Some((c, g)) = glyph(&mut lines)? {
                        glyphs.insert(c, g);
                    }
                }
                "ENDFONT" => break,
                _ => {}
            }
        }
        // PIXEL_SIZE is the em size, so the line is ascent + descent when the font says them
        let ascent = ascent
            .or(bbox.map(|[_, h, _, yoff]| h.saturating_add(yoff)))
            .ok_or("no FONT_ASCENT or FONTBOUNDINGBOX")?;
        let descent = descent.or(bbox.map(|[_, _, _, yoff]| yoff.saturating_neg()));
        let line_height = descent
            .map(|d| ascent.saturating_add(d))
            .or(pixel_size)
            .ok_or("no FONT_DESCENT, FONTBOUNDINGBOX or PIXEL_SIZE")?;
        if line_height <= 0 {
            return Err(format!(
                "the line height must be above 0, got {line_height}"
            ));
        }
        Ok(BdfFont {
            line_height,
            ascent,
            glyphs,
            default: default.and_then(|d| char::from_u32(d as u32)),
        })
    }
}

/// The widest and tallest glyph accepted, in pixels.
const MAX_GLYPH_SIDE: i32 = 4096;

fn ints<const N: usize>(s: &str) -> Option<[i32; N]> {
    let mut out = [0; N];
    let mut it = s.split_whitespace();
    for v in &mut out {
        *v = it.next()?.parse().ok()?;
    }
    Some(out)
}

/// Reads one glyph after `STARTCHAR`, through `ENDCHAR`; `None` for an encoding that is not a character.
fn glyph<'a>(
    lines: &mut impl Iterator<Item = (usize, &'a str)>,
) -> Result<Option<(char, Glyph)>, String> {
    let (mut code, mut advance, mut bbx) = (None, None, None);
    while let Some((n, line)) = lines.next() {
        let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
        let bad = || format!("line {n}: bad {key}: {line}");
        match key {
            "ENCODING" => code = Some(ints::<1>(rest).ok_or_else(bad)?[0]),
            "DWIDTH" => advance = Some(ints::<2>(rest).ok_or_else(bad)?[0]),
            "BBX" => {
                let [w, h, xoff, yoff] = ints::<4>(rest).ok_or_else(bad)?;
                // a crafted file must fail with an error, not abort on a huge allocation
                if !(0..=MAX_GLYPH_SIDE).contains(&w) || !(0..=MAX_GLYPH_SIDE).contains(&h) {
                    return Err(bad());
                }
                bbx = Some((w as u32, h as u32, xoff, yoff));
            }
            "BITMAP" => {
                let (w, h, xoff, yoff) = bbx.ok_or(format!("line {n}: BITMAP before BBX"))?;
                let size = w.checked_mul(h).ok_or(format!("line {n}: bad BBX"))?;
                let mut bits = Vec::with_capacity(size as usize);
                for _ in 0..h {
                    let (n, row) = lines.next().ok_or("the font ends inside a BITMAP")?;
                    let bytes = (0..w.div_ceil(8) as usize)
                        .map(|i| {
                            row.get(2 * i..2 * i + 2)
                                .and_then(|b| u8::from_str_radix(b, 16).ok())
                        })
                        .collect::<Option<Vec<u8>>>()
                        .ok_or(format!("line {n}: bad BITMAP row: {row}"))?;
                    bits.extend((0..w as usize).map(|x| bytes[x / 8] & (0x80 >> (x % 8)) != 0));
                }
                match lines.next() {
                    Some((_, "ENDCHAR")) => {}
                    Some((n, l)) => return Err(format!("line {n}: expected ENDCHAR, got: {l}")),
                    None => return Err("the font ends inside a glyph".into()),
                }
                let c = code
                    .filter(|&c| c >= 0)
                    .and_then(|c| char::from_u32(c as u32));
                let g = Glyph {
                    advance: advance.unwrap_or(w as i32),
                    w,
                    h,
                    xoff,
                    yoff,
                    bits,
                };
                return Ok(c.map(|c| (c, g)));
            }
            _ => {}
        }
    }
    Err("the font ends inside a glyph".into())
}
