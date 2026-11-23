//! One frame's draw commands, as 0.1 `draw.Canvas`: up to 1024 are accepted, text is laid out as it is recorded.
use std::rc::Rc;

use unicode_bidi::{bidi_class, BidiClass};

use super::image::{self, Gray};
use super::pattern::{self, Pattern};
use super::raster::Style;
use super::text::Fonts;
use super::{Frame, TextItem};
use crate::log::py_repr_str;

/// A frame accepts this many draw commands; the device refuses every call past it.
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
    /// 1 black, 0 white.
    pub ink: u8,
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
    /// An image as it was when the command was recorded.
    Image {
        img: Rc<Gray>,
        x: i64,
        y: i64,
        w: u32,
        h: u32,
    },
    /// Counted, draws nothing (`g:image` of something that is not an image, `g:pattern` of an unknown kind).
    Skip,
    /// `g:pattern_rect` / `g:fill_pattern`.
    Checker {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
    Pattern {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        kind: Pattern,
        ink: u8,
    },
}

enum Op {
    Draw(Cmd),
    Lines {
        font: &'static str,
        ink: u8,
        lines: Vec<(i32, i32, String)>,
    },
}

pub struct Canvas {
    pub w: u32,
    pub h: u32,
    fonts: Fonts,
    /// What the last finished frame drew and showed.
    ops: Vec<Op>,
    texts: Vec<TextItem>,
    /// The frame being recorded: a `g` kept by the app and called outside `on_draw` lands here and is dropped by
    /// the next `begin_frame`, as in 0.1.
    rec_ops: Vec<Op>,
    rec_texts: Vec<TextItem>,
    /// Commands accepted in the frame being recorded.
    n: usize,
    /// Commands asked for, refused ones too.
    tried: usize,
    /// `g:image` commands accepted.
    images: usize,
    warnings: Vec<String>,
    bad: Option<String>,
}

/// Python's `round()`: halves go to the even neighbour.
pub(crate) fn py_round(v: f64) -> i32 {
    v.round_ties_even() as i32
}

impl Canvas {
    pub fn new(w: u32, h: u32, fonts: Fonts) -> Self {
        Canvas {
            w,
            h,
            fonts,
            ops: vec![],
            texts: vec![],
            rec_ops: vec![],
            rec_texts: vec![],
            n: 0,
            tried: 0,
            images: 0,
            warnings: vec![],
            bad: None,
        }
    }

    pub fn fonts(&self) -> Fonts {
        self.fonts.clone()
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        (self.w, self.h) = (w, h);
    }

    pub fn begin_frame(&mut self) {
        self.ops.clear();
        self.texts.clear();
        self.rec_ops.clear();
        self.rec_texts.clear();
        self.warnings.clear();
        (self.n, self.tried, self.images) = (0, 0, 0);
        self.bad = None;
    }

    /// Adds a command if the frame has room for it, else refuses it whole (false). Each line of a text box is
    /// one command. `Err` is a value the device cannot draw: the frame fails at `end_frame`.
    pub fn record(&mut self, cmd: Result<Cmd, String>) -> bool {
        if self.n >= CAP {
            self.tried += 1;
            return false;
        }
        let cmd = match cmd {
            Ok(c) => c,
            Err(msg) => {
                (self.n, self.tried) = (self.n + 1, self.tried + 1);
                self.bad.get_or_insert(msg);
                return true;
            }
        };
        let (op, text, cost) = match cmd {
            Cmd::Text { .. } | Cmd::TextBox { .. } => match self.layout(&cmd) {
                Some((op, text)) => {
                    let lines = match &op {
                        Op::Lines { lines, .. } if matches!(cmd, Cmd::TextBox { .. }) => {
                            lines.len()
                        }
                        _ => 1,
                    };
                    (Some(op), Some(text), lines)
                }
                None => (None, None, 1),
            },
            c => (Some(Op::Draw(c)), None, 1),
        };
        self.tried += cost;
        if self.n + cost > CAP {
            return false;
        }
        self.n += cost;
        if let Some(Op::Draw(Cmd::Image { .. })) = op {
            self.images += 1;
        }
        self.rec_texts.extend(text);
        self.rec_ops.extend(op);
        true
    }

    /// Shows the recorded frame; the number of commands asked for, accepted or not.
    pub fn end_frame(&mut self) -> usize {
        self.ops = std::mem::take(&mut self.rec_ops);
        self.texts = std::mem::take(&mut self.rec_texts);
        if self.tried > CAP {
            let msg = format!(
                "{} draw commands in one frame; the device drops everything after {CAP}",
                self.tried
            );
            self.warnings.insert(0, msg);
        }
        self.tried
    }

    /// Commands and `g:image` calls accepted in the frame being recorded (after `end_frame`: the finished one).
    pub fn accepted(&self) -> (usize, usize) {
        (self.n, self.images)
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

    fn layout(&mut self, cmd: &Cmd) -> Option<(Op, TextItem)> {
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
        let (font, f) = (opts.font, self.fonts.clone());
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
                let keep = (h / lh).floor().max(1.0);
                if lines.len() as f64 > keep {
                    lines.truncate(keep as usize);
                }
                let bh = lines.len() as f64 * lh;
                let by = py_round(place(y, bh, h, opts.valign));
                let lines = lines
                    .into_iter()
                    .enumerate()
                    .map(|(i, line)| {
                        let lx = place(x, f64::from(f.measure(font, &line)), w, opts.align);
                        (py_round(lx), by.saturating_add(i as i32 * lh as i32), line)
                    })
                    .collect();
                ((py_round(x), by, py_round(w), bh as i32), lines)
            }
            _ => return None,
        };
        let (x, y, w, h) = item;
        let item = TextItem {
            text: s.clone(),
            x,
            y,
            w,
            h,
            font,
        };
        let op = Op::Lines {
            font,
            ink: opts.ink,
            lines,
        };
        Some((op, item))
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
        self.render_on(Frame::new(self.w, self.h))
    }

    /// The last finished frame drawn over `f` (a layer's previous contents).
    pub fn render_on(&self, mut f: Frame) -> Frame {
        for op in &self.ops {
            match op {
                Op::Lines { font, ink, lines } => {
                    for (x, y, line) in lines {
                        self.fonts.draw(&mut f, font, *x, *y, line, *ink);
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
        Cmd::Checker { x, y, w, h } => pattern::checker(f, x, y, w, h),
        Cmd::Pattern {
            x,
            y,
            w,
            h,
            kind,
            ink,
        } => pattern::fill(f, (x, y, w, h), kind, ink),
        Cmd::Image {
            ref img,
            x,
            y,
            w,
            h,
        } => image::paste(f, img, x, y, w, h),
        _ => {}
    }
}
