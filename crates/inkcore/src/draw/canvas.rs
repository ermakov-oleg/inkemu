//! One frame's draw commands, as 0.1 `draw.Canvas`: the first 1024 are kept, text is laid out as it is recorded.
use unicode_bidi::{bidi_class, BidiClass};

use super::raster::Style;
use super::text::Fonts;
use super::{pattern, Frame, TextItem};
use crate::log::py_repr_str;

/// The device drops every draw command of a frame after this many.
pub const CAP: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Middle,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextOpts {
    pub font: &'static str,
    pub align: Align,
    pub valign: Align,
}

/// A draw command with its values already checked.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Clear {
        ink: u8,
    },
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        radius: f64,
        style: Style,
    },
    Line {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        width: i32,
        ink: u8,
    },
    Circle {
        x: f64,
        y: f64,
        r: f64,
        arc: Option<(f64, f64)>,
        style: Style,
    },
    Polygon {
        pts: Vec<(f64, f64)>,
        style: Style,
    },
    Polyline {
        pts: Vec<(f64, f64)>,
        width: i32,
        ink: u8,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        opts: TextOpts,
    },
    TextBox {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        text: String,
        opts: TextOpts,
    },
    Image,
    Pattern {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
}

enum Op {
    Draw(Cmd),
    Lines {
        font: &'static str,
        lines: Vec<(i32, i32, String)>,
    },
}

pub struct Canvas {
    pub w: u32,
    pub h: u32,
    fonts: Fonts,
    ops: Vec<Op>,
    n: usize,
    texts: Vec<TextItem>,
    warnings: Vec<String>,
    bad: Option<String>,
}

/// Python's `round()`: halves go to the even neighbour.
pub(crate) fn py_round(v: f64) -> i32 {
    v.round_ties_even() as i32
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        Canvas {
            w,
            h,
            fonts: Fonts::new(),
            ops: vec![],
            n: 0,
            texts: vec![],
            warnings: vec![],
            bad: None,
        }
    }

    pub fn fonts(&self) -> Fonts {
        self.fonts
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        (self.w, self.h) = (w, h);
    }

    pub fn begin_frame(&mut self) {
        self.ops.clear();
        self.texts.clear();
        self.warnings.clear();
        self.n = 0;
        self.bad = None;
    }

    /// Counts one command; only the first `CAP` are drawn, checked and laid out. `Err` is a value the device
    /// cannot draw: the frame fails at `end_frame`.
    pub fn record(&mut self, cmd: Result<Cmd, String>) {
        self.n += 1;
        if self.n > CAP {
            return;
        }
        let cmd = match cmd {
            Ok(c) => c,
            Err(msg) => {
                self.bad.get_or_insert(msg);
                return;
            }
        };
        let op = match cmd {
            Cmd::Text { .. } | Cmd::TextBox { .. } => match self.layout(&cmd) {
                Some(op) => op,
                None => return,
            },
            c => Op::Draw(c),
        };
        self.ops.push(op);
    }

    /// The number of commands recorded this frame, kept or not.
    pub fn end_frame(&mut self) -> usize {
        if self.n > CAP {
            let msg = format!(
                "{} draw commands in one frame; the device drops everything after {CAP}",
                self.n
            );
            self.warnings.insert(0, msg);
        }
        self.n
    }

    /// Warnings of this frame, oldest first.
    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }

    /// The first value in this frame that the device cannot draw.
    pub fn take_bad(&mut self) -> Option<String> {
        self.bad.take()
    }

    pub fn texts(&self) -> Vec<TextItem> {
        self.texts.clone()
    }

    fn layout(&mut self, cmd: &Cmd) -> Option<Op> {
        let (op, s, opts) = match cmd {
            Cmd::Text { text, opts, .. } => ("text", text, opts),
            Cmd::TextBox { text, opts, .. } => ("text_box", text, opts),
            _ => return None,
        };
        if s.chars()
            .any(|c| matches!(bidi_class(c), BidiClass::R | BidiClass::AL))
        {
            self.warnings.push(format!(
                "g:{op} with RTL text is not drawn by the device: {}",
                py_repr_str(s)
            ));
            return None;
        }
        let (font, f) = (opts.font, self.fonts);
        let lh = f64::from(f.line_height(font));
        let place = |at: f64, size: f64, room: f64, a: Align| match a {
            Align::Middle => at + (room - size) / 2.0,
            Align::End => at + room - size,
            Align::Start => at,
        };
        let (item, lines) = match *cmd {
            Cmd::Text { x, y, .. } => {
                let bw = f.measure(font, s);
                let bx = py_round(place(x, f64::from(bw), 0.0, opts.align));
                let by = py_round(place(y, lh, 0.0, opts.valign));
                let item = (bx, by, bw, lh as i32);
                (item, vec![(bx, by, s.clone())])
            }
            Cmd::TextBox { x, y, w, h, .. } => {
                let mut lines = self.wrap(font, s, w);
                lines.truncate(((h / lh).floor() as i64).max(1) as usize);
                let bh = lines.len() as f64 * lh;
                let by = py_round(place(y, bh, h, opts.valign));
                let lines = lines
                    .into_iter()
                    .enumerate()
                    .map(|(i, line)| {
                        let lx = place(x, f64::from(f.measure(font, &line)), w, opts.align);
                        (py_round(lx), by + i as i32 * lh as i32, line)
                    })
                    .collect();
                ((py_round(x), by, py_round(w), bh as i32), lines)
            }
            _ => return None,
        };
        let (x, y, w, h) = item;
        self.texts.push(TextItem {
            text: s.clone(),
            x,
            y,
            w,
            h,
            font,
        });
        Some(Op::Lines { font, lines })
    }

    /// 0.1 `_wrap`: words on spaces, a word wider than `width` split by characters; `\n` starts a new line.
    pub fn wrap(&self, font: &str, s: &str, width: f64) -> Vec<String> {
        let fits = |t: &str| f64::from(self.fonts.measure(font, t)) <= width;
        let mut out = vec![];
        for para in s.split('\n') {
            let mut cur = String::new();
            for word in para.split(' ') {
                let cand = if cur.is_empty() {
                    word.to_string()
                } else {
                    format!("{cur} {word}")
                };
                if fits(&cand) {
                    cur = cand;
                    continue;
                }
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                let mut word: Vec<char> = word.chars().collect();
                while word.len() > 1 && !fits(&word.iter().collect::<String>()) {
                    let mut k = word.len() - 1;
                    while k > 1 && !fits(&word[..k].iter().collect::<String>()) {
                        k -= 1;
                    }
                    out.push(word[..k].iter().collect());
                    word.drain(..k);
                }
                cur = word.into_iter().collect();
            }
            out.push(cur);
        }
        out
    }

    pub fn render(&self) -> Frame {
        let mut f = Frame::new(self.w, self.h);
        for op in &self.ops {
            match op {
                Op::Lines { font, lines } => {
                    for (x, y, line) in lines {
                        self.fonts.draw(&mut f, font, *x, *y, line);
                    }
                }
                Op::Draw(cmd) => draw(&mut f, cmd),
            }
        }
        f
    }
}

fn draw(f: &mut Frame, cmd: &Cmd) {
    match *cmd {
        Cmd::Clear { ink } => f.clear(ink),
        Cmd::Rect {
            x,
            y,
            w,
            h,
            radius,
            style,
        } if w > 0.0 && h > 0.0 => f.rect(x, y, w, h, radius, style),
        Cmd::Line {
            x0,
            y0,
            x1,
            y1,
            width,
            ink,
        } => f.line(x0, y0, x1, y1, width, ink),
        Cmd::Circle {
            x,
            y,
            r,
            arc,
            style,
        } => f.circle(x, y, r, arc, style),
        Cmd::Polygon { ref pts, style } if pts.len() >= 3 => f.polygon(pts, style),
        Cmd::Polyline {
            ref pts,
            width,
            ink,
        } if pts.len() >= 2 => f.polyline(pts, width, ink),
        Cmd::Pattern { x, y, w, h } => pattern::fill(f, x, y, w, h),
        _ => {}
    }
}
