//! One frame's draw commands, as 0.1 `draw.Canvas`: up to 1024 are accepted, text is laid out as it is recorded.
use std::rc::Rc;

use super::image::{self, Gray, Look};
use super::pattern::{self, Pattern};
use super::raster::Style;
use super::text::{Fonts, LayoutOpts, Overflow, Paint};
use super::{Depth, Frame, TextItem};

/// A frame accepts this many draw commands; the device refuses every call past it.
pub const CAP: usize = 1024;
/// All text of one frame, in bytes (§7).
pub const FRAME_TEXT_BYTES: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Middle,
    End,
    /// `valign = "baseline"`: `y` is the baseline (`text_box` treats it as the top).
    Baseline,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextOpts {
    pub font: &'static str,
    pub align: Align,
    pub valign: Align,
    /// 0 paper … 15 ink.
    pub color: u8,
    /// The style, and for `text_box` the wrapping options.
    pub layout: LayoutOpts,
}

/// A draw command with its values already checked. Colours stay 0..15 (`style.ink` too) until the frame is
/// rendered, since a later `g:image{gray = true}` can make the whole frame gray.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Clear {
        color: u8,
        /// A normal frame clears black for any colour but 0, as 0.1 did; a gray frame takes `color`'s step.
        black: bool,
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
        color: u8,
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
        color: u8,
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
    /// An image (its `src` region already cut out) as it was when the command was recorded.
    Image {
        img: Rc<Gray>,
        x: i64,
        y: i64,
        w: u32,
        h: u32,
        look: Look,
        /// `gray = true` on the screen's `g`.
        gray: bool,
    },
    Pattern {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        kind: Pattern,
        color: u8,
    },
}

enum Op {
    Draw(Cmd),
    /// Lines of text: the left and top of each line box, the text, its width and whether `paint.clip` cuts it;
    /// `paint.ink` is the colour.
    Lines {
        font: &'static str,
        paint: Paint,
        lines: Vec<(i32, i32, String, i32, bool)>,
    },
}

pub struct Canvas {
    pub w: u32,
    pub h: u32,
    fonts: Fonts,
    /// What the last finished frame drew and showed.
    ops: Vec<Op>,
    texts: Vec<TextItem>,
    /// The frame being recorded.
    rec_ops: Vec<Op>,
    rec_texts: Vec<TextItem>,
    /// Commands accepted in the frame being recorded.
    n: usize,
    /// Commands asked for, refused ones too.
    tried: usize,
    /// `g:image` commands accepted.
    images: usize,
    /// A `g:image{gray = true}` was accepted in the frame being recorded, and in the last finished one.
    rec_gray: bool,
    gray: bool,
    /// Bytes of text accepted in the frame being recorded.
    text_bytes: usize,
    warnings: Vec<String>,
    bad: Option<String>,
    /// Inside the callback that owns this canvas's `g`; outside it every `g` call is `nil, "inactive"` (§6).
    active: bool,
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
            rec_gray: false,
            gray: false,
            text_bytes: 0,
            warnings: vec![],
            bad: None,
            active: false,
        }
    }

    pub fn fonts(&self) -> Fonts {
        self.fonts.clone()
    }

    pub fn active(&self) -> bool {
        self.active
    }

    /// Opens or closes the canvas to its `g`; the previous state, for nested callbacks to restore.
    pub fn set_active(&mut self, on: bool) -> bool {
        std::mem::replace(&mut self.active, on)
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
        (self.n, self.tried, self.images, self.text_bytes) = (0, 0, 0, 0);
        (self.rec_gray, self.gray) = (false, false);
        self.bad = None;
    }

    /// Adds a command if the frame has room for it, else refuses it whole with `limit_exceeded`. Each line of a
    /// text box is one command, and each underline or strikethrough of a line one more. `Err` is a value the
    /// device cannot draw: the frame fails at `end_frame`.
    pub fn record(&mut self, cmd: Result<Cmd, String>) -> Result<(), &'static str> {
        let cmd = match cmd {
            Ok(c) => c,
            Err(msg) => {
                self.tried += 1;
                if self.n >= CAP {
                    return Err("limit_exceeded");
                }
                self.n += 1;
                self.bad.get_or_insert(msg);
                return Ok(());
            }
        };
        let bytes = match &cmd {
            Cmd::Text { text, .. } | Cmd::TextBox { text, .. } => text.len(),
            _ => 0,
        };
        if self.text_bytes + bytes > FRAME_TEXT_BYTES {
            return Err("limit_exceeded");
        }
        let (op, text, cost) = match cmd {
            Cmd::Text { .. } | Cmd::TextBox { .. } => self.layout(&cmd),
            c => (Some(Op::Draw(c)), None, 1),
        };
        self.tried += cost;
        if self.n + cost > CAP {
            return Err("limit_exceeded");
        }
        self.n += cost;
        self.text_bytes += bytes;
        if let Some(Op::Draw(Cmd::Image { gray, .. })) = op {
            self.images += 1;
            self.rec_gray |= gray;
        }
        self.rec_texts.extend(text);
        self.rec_ops.extend(op);
        Ok(())
    }

    pub fn warn(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    /// Shows the recorded frame; the number of commands asked for, accepted or not.
    pub fn end_frame(&mut self) -> usize {
        self.ops = std::mem::take(&mut self.rec_ops);
        self.texts = std::mem::take(&mut self.rec_texts);
        self.gray = std::mem::take(&mut self.rec_gray);
        if self.tried > CAP {
            let msg = format!(
                "{} draw commands in one frame; the device refuses every call past {CAP}",
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

    /// The last finished frame is a gray frame (§3).
    pub fn gray(&self) -> bool {
        self.gray
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

    /// The lines of a text command, what `texts()` lists of it, and the commands it costs.
    fn layout(&self, cmd: &Cmd) -> (Option<Op>, Option<TextItem>, usize) {
        let (s, opts) = match cmd {
            Cmd::Text { text, opts, .. } | Cmd::TextBox { text, opts, .. } => (text, opts),
            _ => return (None, None, 0),
        };
        let (font, f, style) = (opts.font, &self.fonts, opts.layout.style);
        let lh = f.line_height(font);
        let place = |at: f64, size: f64, room: f64, a: Align| match a {
            Align::Middle => at + (room - size) / 2.0,
            Align::End => at + room - size,
            Align::Start | Align::Baseline => at,
        };
        // a stroke under a line of width 0 is not drawn
        let cost = |w: i32| 1 + if w > 0 { style.strokes() } else { 0 };
        let mut paint = Paint {
            ink: opts.color,
            style,
            clip: None,
        };
        let (item, lines) = match *cmd {
            Cmd::Text { x, y, .. } => {
                let bw = f.measure_with(font, s, style);
                // §7: the line box starts at `x - width // 2` and `y - line_height // 2` when centred
                let anchor = |at: f64, size: i32, a: Align| {
                    let at = py_round(at);
                    match a {
                        Align::Middle => at.saturating_sub(size.div_euclid(2)),
                        Align::End => at.saturating_sub(size),
                        Align::Start => at,
                        Align::Baseline => at.saturating_sub(f.ascent(font)),
                    }
                };
                let (bx, by) = (anchor(x, bw, opts.align), anchor(y, lh, opts.valign));
                ((bx, by, bw, lh), vec![(bx, by, s.clone(), bw, false)])
            }
            Cmd::TextBox { x, y, w, h, .. } => {
                let lay = f.layout(font, s, w, h, &opts.layout);
                if lay.lines.is_empty() {
                    return (None, None, 0);
                }
                if opts.layout.overflow == Overflow::Clip {
                    paint.clip = Some((py_round(x), py_round(x + w)));
                }
                let by = py_round(place(y, f64::from(lay.height), h, opts.valign));
                let step = lh.saturating_add(opts.layout.line_gap);
                let lines = lay
                    .lines
                    .into_iter()
                    .enumerate()
                    .map(|(i, l)| {
                        let lx = py_round(place(x, f64::from(l.width), w, opts.align));
                        let ly = by.saturating_add((i as i32).saturating_mul(step));
                        // only a line that cannot wrap is cut; ink overhanging a line that fits stays
                        let cut = f64::from(l.width) > w;
                        (lx, ly, l.text, l.width, cut)
                    })
                    .collect();
                ((py_round(x), by, py_round(w), lay.height), lines)
            }
            _ => return (None, None, 0),
        };
        let cost = lines.iter().map(|l| cost(l.3)).sum();
        let (x, y, w, h) = item;
        let item = TextItem {
            text: s.clone(),
            x,
            y,
            w,
            h,
            font,
        };
        let op = Op::Lines { font, paint, lines };
        (Some(op), Some(item), cost)
    }

    /// Word-wrapped lines of at most `width` pixels, every line kept: the host's plates.
    pub fn wrap(&self, font: &str, s: &str, width: f64) -> Vec<String> {
        self.fonts.wrap(font, s, width)
    }

    /// The last finished frame in 1 bit, gray or not.
    pub fn render(&self) -> Frame {
        self.render_on(Frame::new(self.w, self.h))
    }

    /// The last finished frame in `depth`.
    pub fn render_in(&self, depth: Depth) -> Frame {
        self.draw_on(Frame::new(self.w, self.h), depth)
    }

    /// The last finished frame drawn over `f` (a layer's previous contents) in 1 bit.
    pub fn render_on(&self, f: Frame) -> Frame {
        self.draw_on(f, Depth::Bits)
    }

    fn draw_on(&self, mut f: Frame, depth: Depth) -> Frame {
        for op in &self.ops {
            match op {
                Op::Lines { font, paint, lines } => {
                    for (x, y, line, w, cut) in lines {
                        let p = Paint {
                            ink: depth.of(paint.ink),
                            clip: paint.clip.filter(|_| *cut),
                            ..*paint
                        };
                        self.fonts.draw_styled(&mut f, font, (*x, *y), line, p);
                        self.fonts.draw_strokes(&mut f, font, (*x, *y, *w), p);
                    }
                }
                Op::Draw(cmd) => draw(&mut f, cmd, depth),
            }
        }
        f
    }
}

fn draw(f: &mut Frame, cmd: &Cmd, depth: Depth) {
    let style = |st: Style| Style {
        ink: depth.of(st.ink),
        ..st
    };
    match *cmd {
        Cmd::Clear { color, black } => f.clear(match depth {
            Depth::Bits => u8::from(black),
            Depth::Gray => depth.of(color),
        }),
        Cmd::Rect {
            x,
            y,
            w,
            h,
            radius,
            style: st,
        } if w > 0.0 && h > 0.0 => f.rect(x, y, w, h, radius, style(st)),
        Cmd::Line {
            x0,
            y0,
            x1,
            y1,
            width,
            color,
        } => f.line(x0, y0, x1, y1, width, depth.of(color)),
        Cmd::Circle {
            x,
            y,
            r,
            arc,
            style: st,
        } => f.circle(x, y, r, arc, style(st)),
        Cmd::Polygon { ref pts, style: st } if pts.len() >= 3 => f.polygon(pts, style(st)),
        Cmd::Polyline {
            ref pts,
            width,
            color,
        } if pts.len() >= 2 => f.polyline(pts, width, depth.of(color)),
        Cmd::Pattern {
            x,
            y,
            w,
            h,
            kind,
            color,
        } => pattern::fill(f, (x, y, w, h), kind, depth.of(color)),
        Cmd::Image {
            ref img,
            x,
            y,
            w,
            h,
            look,
            ..
        } => image::paste(f, img, (x, y), (w, h), look, depth),
        _ => {}
    }
}
