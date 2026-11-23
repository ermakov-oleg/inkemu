//! The screen: draw commands, 1-bit rendering, text and the host's overlay plates.
pub mod bdf;
pub mod canvas;
pub mod image;
pub mod pattern;
pub(crate) mod plates;
pub mod raster;
pub mod text;

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value};
use unicode_bidi::{bidi_class, BidiClass};

pub use canvas::{Align, Canvas, Cmd, TextOpts, CAP, FRAME_TEXT_BYTES};
pub use pattern::Pattern;
pub use raster::Style;

use crate::ctx::object::{method, new_object, Method};
use crate::emu::{with, Shared};
use crate::log::py_repr_str;
use text::{LayoutOpts, Overflow, TextStyle, Wrap};

/// The bytes one `g:text`, `g:text_box` or `font:measure` call takes (§7).
pub const CALL_TEXT_BYTES: usize = 8192;

#[derive(Clone, Debug, PartialEq)]
pub struct TextItem {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub font: &'static str,
}

/// One byte per pixel, row by row: 0 white, 1 black.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Frame {
    /// A white frame.
    pub fn new(w: u32, h: u32) -> Self {
        Frame {
            w,
            h,
            px: vec![0; w as usize * h as usize],
        }
    }

    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.px[(y * self.w + x) as usize]
    }

    /// Rows packed 8 pixels per byte, MSB first, set bit = white: PIL's raw mode "1".
    pub fn packed(&self) -> Vec<u8> {
        let (w, h) = (self.w as usize, self.h as usize);
        let stride = w.div_ceil(8);
        let mut out = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                if self.px[y * w + x] == 0 {
                    out[y * stride + x / 8] |= 0x80 >> (x % 8);
                }
            }
        }
        out
    }

    /// Sets one pixel; outside the frame does nothing.
    pub fn set(&mut self, x: i32, y: i32, v: u8) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            self.px[(y as u32 * self.w + x as u32) as usize] = v;
        }
    }
}

/// Argument kinds of the recorded `g` methods, checked as 0.1 did: n number, t table, s string or number,
/// a any value.
const SPECS: [(&str, &str); 14] = [
    ("clear", ""),
    ("rect", "nnnn"),
    ("round_rect", "nnnn"),
    ("line", "nnnn"),
    ("circle", "nnn"),
    ("arc", "nnnnn"),
    ("polygon", "t"),
    ("polyline", "t"),
    ("text", "nns"),
    ("text_box", "nnnns"),
    ("image", "ann"),
    ("pattern_rect", "nnnn"),
    ("fill_pattern", "nnnn"),
    ("pattern", "nnnn"),
];

/// Where a `g` draws: the screen, or a layer's own canvas.
#[derive(Clone)]
pub(crate) enum Target {
    Screen,
    Layer(Rc<RefCell<Canvas>>),
}

/// Runs `f` on the target canvas. The borrow ends when `f` returns: never call into Lua inside `f`.
fn on_canvas<R>(st: &Shared, t: &Target, f: impl FnOnce(&mut Canvas) -> R) -> R {
    match t {
        Target::Screen => with(st, |s| f(&mut s.canvas)),
        Target::Layer(c) => f(&mut c.borrow_mut()),
    }
}

/// The `g` passed to `on_draw` (Screen) or to a `layer:draw` callback.
pub(crate) fn new_g(lua: &Lua, st: &Shared, target: Target) -> mlua::Result<Table> {
    let mut methods: Vec<(&str, Method)> = vec![];
    for (name, spec) in SPECS {
        let (st, target) = (st.clone(), target.clone());
        methods.push((
            name,
            method(move |lua, args| {
                let a = args.into_vec();
                check(name, spec, &a)?;
                let text = match name {
                    "text" => Some(&a[2]),
                    "text_box" => Some(&a[4]),
                    _ => None,
                };
                let text = match text.map(|v| draw_text(lua, name, v)) {
                    Some(Err((code, warning))) => {
                        if let Some(w) = warning {
                            on_canvas(&st, &target, |c| c.warn(w));
                        }
                        return (Value::Nil, code).into_lua_multi(lua);
                    }
                    Some(Ok(s)) => s,
                    None => String::new(),
                };
                let cmd = if name == "image" {
                    image_command(&st, &a)
                } else {
                    let api = with(&st, |s| s.app_api);
                    command(name, &a, text, api)
                };
                match on_canvas(&st, &target, |c| c.record(cmd)) {
                    Ok(()) => Ok(MultiValue::new()),
                    Err(code) => (Value::Nil, code).into_lua_multi(lua),
                }
            }),
        ));
    }
    let size = {
        let (st, target) = (st.clone(), target.clone());
        method(move |lua, _| on_canvas(&st, &target, |c| (c.w, c.h)).into_lua_multi(lua))
    };
    let measure = {
        let (st, target) = (st.clone(), target);
        method(move |lua, args| {
            let a = args.into_vec();
            let s = match a.first() {
                Some(v) => text_of(lua, v),
                None => "nil".into(),
            };
            let font = font_of(&opts_at(&a, 1));
            let fonts = on_canvas(&st, &target, |c| c.fonts());
            fonts.measure(font, &s).into_lua_multi(lua)
        })
    };
    methods.push(("size", size));
    methods.push(("measure", measure.clone()));
    methods.push(("measure_text", measure));
    new_object(lua, "g", &methods)
}

fn lua_type(v: &Value) -> &'static str {
    match v {
        Value::Integer(_) | Value::Number(_) => "number",
        Value::LightUserData(_) | Value::UserData(_) => "userdata",
        v => v.type_name(),
    }
}

fn check(name: &str, spec: &str, a: &[Value]) -> mlua::Result<()> {
    for (i, k) in spec.chars().enumerate() {
        let v = a.get(i).unwrap_or(&Value::Nil);
        let num = matches!(v, Value::Integer(_) | Value::Number(_));
        let ok = match k {
            'n' => num,
            't' => v.is_table(),
            's' => num || v.is_string(),
            _ => !v.is_nil(),
        };
        if !ok {
            let want = match k {
                'n' => "number",
                't' => "table",
                's' => "string",
                _ => "value",
            };
            let got = if v.is_nil() { "no value" } else { lua_type(v) };
            return Err(mlua::Error::runtime(format!(
                "bad argument #{} to '{name}' ({want} expected, got {got})",
                i + 1
            )));
        }
    }
    Ok(())
}

/// Python `repr` of a value 0.1 rejected.
fn show(v: &Value) -> String {
    match v {
        Value::String(s) => py_repr_str(&s.to_string_lossy()),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) if n.is_nan() => "nan".into(),
        Value::Number(n) if n.is_infinite() => if *n > 0.0 { "inf" } else { "-inf" }.into(),
        Value::Number(n) => n.to_string(),
        Value::Boolean(b) => if *b { "True" } else { "False" }.into(),
        v => lua_type(v).into(),
    }
}

fn as_num(v: &Value) -> Option<f64> {
    match *v {
        Value::Integer(i) => Some(f64::from(i)),
        Value::Number(n) => Some(n as f64),
        _ => None,
    }
}

/// Positional numbers: the type was checked on the call; the device cannot place infinity or NaN.
fn nums<const N: usize>(op: &str, a: &[Value], names: [&str; N]) -> Result<[f64; N], String> {
    let mut out = [0.0; N];
    for (i, name) in names.into_iter().enumerate() {
        let v = a.get(i).unwrap_or(&Value::Nil);
        out[i] = as_num(v)
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("{op}: {name} must be a finite number, got {}", show(v)))?;
    }
    Ok(out)
}

fn opts_at(a: &[Value], i: usize) -> Option<Table> {
    match a.get(i) {
        Some(Value::Table(t)) => Some(t.clone()),
        _ => None,
    }
}

fn get(o: &Option<Table>, key: &str) -> Value {
    o.as_ref()
        .and_then(|t| t.raw_get(key).ok())
        .unwrap_or(Value::Nil)
}

/// Python truthiness of the value 0.1 saw: nil, false, 0, "" and {} are false.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Nil | Value::Boolean(false) | Value::Integer(0) => false,
        Value::Number(n) => *n != 0.0,
        Value::String(s) => !s.as_bytes().is_empty(),
        Value::Table(t) => !t.is_empty(),
        _ => true,
    }
}

/// 1 black, 0 white. The device's scale is 0 paper to 15 ink, drawn as a threshold: below 8 is white. A colour
/// that is not a number keeps its 0.1 meaning: `false` (Python `== 0`) is white, anything else black.
fn ink(o: &Option<Table>) -> u8 {
    match get(o, "color") {
        Value::Boolean(false) => 0,
        v => as_num(&v).map_or(1, |n| u8::from(n >= 8.0)),
    }
}

/// `line_width`, else `width`, as Python `int()`, at least 1.
fn line_width(op: &str, o: &Option<Table>) -> Result<i32, String> {
    let (key, v) = match get(o, "line_width") {
        Value::Nil => ("width", get(o, "width")),
        v => ("line_width", v),
    };
    let n = match &v {
        Value::Nil => Some(1),
        Value::Boolean(b) => Some(i64::from(*b)),
        Value::String(s) => s.to_string_lossy().trim().parse::<i64>().ok(),
        v => as_num(v)
            .filter(|n| n.is_finite())
            .map(|n| n.trunc() as i64),
    };
    n.map(|n| n.clamp(1, i64::from(i32::MAX)) as i32)
        .ok_or_else(|| format!("{op}: {key} must be a whole number, got {}", show(&v)))
}

fn style(op: &str, o: &Option<Table>) -> Result<Style, String> {
    Ok(Style {
        ink: ink(o),
        fill: truthy(&get(o, "fill")),
        width: line_width(op, o)?,
    })
}

fn radius(op: &str, v: &Value, default: f64) -> Result<f64, String> {
    match v {
        Value::Nil => Ok(default),
        Value::Boolean(b) => Ok(f64::from(u8::from(*b))),
        v => as_num(v)
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("{op}: radius must be a finite number, got {}", show(v))),
    }
}

fn font_of(o: &Option<Table>) -> &'static str {
    let name = match get(o, "font") {
        Value::Table(t) => t.raw_get::<Value>("name").unwrap_or(Value::Nil),
        v => v,
    };
    match name {
        Value::String(s) => text::font_name(&s.to_string_lossy()),
        _ => "medium",
    }
}

fn text_opts(o: &Option<Table>, api: (u32, u32)) -> TextOpts {
    let pick = |key: &str, mid: &str, end: &str| match get(o, key) {
        Value::String(s) if s == mid => Align::Middle,
        Value::String(s) if s == end => Align::End,
        Value::String(s) if key == "valign" && s == "baseline" => Align::Baseline,
        _ => Align::Start,
    };
    TextOpts {
        font: font_of(o),
        align: pick("align", "center", "right"),
        valign: pick("valign", "middle", "bottom"),
        ink: ink(o),
        layout: layout_opts(o, api),
    }
}

/// The text options a package declaring `api` gets: bold and tabular from 2.16, italic, underline and strike
/// from 2.28. Unknown `wrap`/`overflow` values keep the default; `line_gap` is clamped to 0..255.
pub(crate) fn layout_opts(o: &Option<Table>, api: (u32, u32)) -> LayoutOpts {
    let on = |key: &str| !matches!(get(o, key), Value::Nil | Value::Boolean(false));
    let (v216, v228) = (api >= (2, 16), api >= (2, 28));
    let word = |key: &str| match get(o, key) {
        Value::String(s) => s.to_string_lossy(),
        _ => String::new(),
    };
    LayoutOpts {
        wrap: match word("wrap").as_str() {
            "char" => Wrap::Char,
            "none" => Wrap::None,
            _ => Wrap::Word,
        },
        overflow: match word("overflow").as_str() {
            "ellipsis" => Overflow::Ellipsis,
            "visible" => Overflow::Visible,
            _ => Overflow::Clip,
        },
        line_gap: as_num(&get(o, "line_gap"))
            .filter(|n| n.is_finite())
            .map_or(0, |n| n.trunc().clamp(0.0, 255.0) as i32),
        style: TextStyle {
            bold: v216 && on("bold"),
            tabular: v216 && on("tabular"),
            italic: v228 && on("italic"),
            underline: v228 && on("underline"),
            strike: v228 && on("strike"),
        },
        kinsoku: v216,
    }
}

/// Text as the device takes it: at most `max` bytes (else `limit_exceeded`), valid UTF-8 (else
/// `invalid_argument`).
pub(crate) fn utf8_text(bytes: Vec<u8>, max: usize) -> Result<String, &'static str> {
    if bytes.len() > max {
        return Err("limit_exceeded");
    }
    String::from_utf8(bytes).map_err(|_| "invalid_argument")
}

/// Whether `s` holds a right-to-left character, which the device does not lay out.
pub(crate) fn has_rtl(s: &str) -> bool {
    s.chars()
        .any(|c| matches!(bidi_class(c), BidiClass::R | BidiClass::AL))
}

/// The text of a `g:text`/`g:text_box` call, or its refusal code and a warning.
fn draw_text(lua: &Lua, op: &str, v: &Value) -> Result<String, (&'static str, Option<String>)> {
    let bytes = match v {
        Value::String(s) => s.as_bytes().to_vec(),
        v => text_of(lua, v).into_bytes(),
    };
    let s = utf8_text(bytes, CALL_TEXT_BYTES).map_err(|code| {
        let w = (code == "invalid_argument")
            .then(|| format!("g:{op} with invalid UTF-8 is not drawn by the device"));
        (code, w)
    })?;
    if has_rtl(&s) {
        let w = format!(
            "g:{op} with RTL text is not drawn by the device: {}",
            py_repr_str(&s)
        );
        return Err(("invalid_argument", Some(w)));
    }
    Ok(s)
}

/// Python `str()` of a checked string-or-number argument, with Lua's number formatting.
fn text_of(lua: &Lua, v: &Value) -> String {
    match lua.coerce_string(v.clone()) {
        Ok(Some(s)) => s.to_string_lossy(),
        _ => v.to_string().unwrap_or_default(),
    }
}

/// 0.1 `_points`: flat numbers, `{x, y}` pairs or `{x=, y=}` tables, mixed; fewer than `min` points draw nothing.
fn points(op: &str, t: &Table, min: usize) -> Result<Vec<(f64, f64)>, String> {
    let mut flat = vec![];
    for i in 1..=t.raw_len() {
        match t.raw_get::<Value>(i).unwrap_or(Value::Nil) {
            Value::Table(p) if p.raw_len() > 0 => flat.extend(
                (1..=p.raw_len().min(2)).map(|j| p.raw_get::<Value>(j).unwrap_or(Value::Nil)),
            ),
            Value::Table(p) => {
                for k in ["x", "y"] {
                    flat.push(match p.raw_get::<Value>(k).unwrap_or(Value::Nil) {
                        Value::Nil => Value::Integer(0),
                        v => v,
                    });
                }
            }
            v => flat.push(v),
        }
    }
    let flat = flat
        .iter()
        .map(|v| {
            as_num(v)
                .filter(|n| n.is_finite())
                .ok_or_else(|| format!("{op}: points must be finite numbers, got {}", show(v)))
        })
        .collect::<Result<Vec<f64>, String>>()?;
    if flat.len() < 2 * min {
        return Ok(vec![]);
    }
    if flat.len() % 2 == 1 {
        return Err(format!(
            "{op}: points must be x, y pairs, got {} numbers",
            flat.len()
        ));
    }
    Ok(flat.chunks(2).map(|p| (p[0], p[1])).collect())
}

/// The checked command for one `g` call (`text` already checked); `Err` is an option value the device cannot
/// draw.
fn command(op: &str, a: &[Value], text: String, api: (u32, u32)) -> Result<Cmd, String> {
    let o = |i: usize| opts_at(a, i);
    Ok(match op {
        "clear" => {
            let black = match a.first() {
                None | Some(Value::Table(_)) => false,
                Some(Value::Integer(0) | Value::Boolean(false)) => false,
                Some(Value::Number(n)) => *n != 0.0,
                Some(_) => true,
            };
            Cmd::Clear {
                ink: u8::from(black),
            }
        }
        "rect" | "round_rect" => {
            let [x, y, w, h] = nums(op, a, ["x", "y", "w", "h"])?;
            let positional = op == "round_rect" && a.len() > 4 && !a[4].is_table();
            let (radius, opts) = if positional {
                (radius(op, &a[4], 0.0)?, o(5))
            } else {
                let default = if op == "round_rect" { 8.0 } else { 0.0 };
                (radius(op, &get(&o(4), "radius"), default)?, o(4))
            };
            Cmd::Rect {
                x,
                y,
                w,
                h,
                radius,
                style: style(op, &opts)?,
            }
        }
        "line" => {
            let [x0, y0, x1, y1] = nums(op, a, ["x1", "y1", "x2", "y2"])?;
            Cmd::Line {
                x0,
                y0,
                x1,
                y1,
                width: line_width(op, &o(4))?,
                ink: ink(&o(4)),
            }
        }
        "circle" => {
            let [x, y, r] = nums(op, a, ["x", "y", "r"])?;
            Cmd::Circle {
                x,
                y,
                r,
                arc: None,
                style: style(op, &o(3))?,
            }
        }
        "arc" => {
            let [x, y, r, a0, a1] = nums(op, a, ["x", "y", "r", "start", "end"])?;
            Cmd::Circle {
                x,
                y,
                r,
                arc: Some((a0, a1)),
                style: style(op, &o(5))?,
            }
        }
        "polygon" | "polyline" => {
            let pts = match a.first() {
                Some(Value::Table(t)) => points(op, t, if op == "polygon" { 3 } else { 2 })?,
                _ => vec![],
            };
            let st = style(op, &o(1))?;
            if op == "polygon" {
                Cmd::Polygon { pts, style: st }
            } else {
                Cmd::Polyline {
                    pts,
                    width: st.width,
                    ink: st.ink,
                }
            }
        }
        "text" => {
            let [x, y] = nums(op, a, ["x", "y"])?;
            Cmd::Text {
                x,
                y,
                text,
                opts: text_opts(&o(3), api),
            }
        }
        "text_box" => {
            let [x, y, w, h] = nums(op, a, ["x", "y", "w", "h"])?;
            Cmd::TextBox {
                x,
                y,
                w,
                h,
                text,
                opts: text_opts(&o(5), api),
            }
        }
        "pattern_rect" | "fill_pattern" => {
            let [x, y, w, h] = nums(op, a, ["x", "y", "w", "h"])?;
            let [x, y, w, h] = [x, y, w, h].map(|v| v.trunc() as i32);
            Cmd::Checker { x, y, w, h }
        }
        "pattern" => {
            let [x, y, w, h] = nums(op, a, ["x", "y", "w", "h"])?;
            let [x, y, w, h] = [x, y, w, h].map(|v| v.trunc() as i32);
            let o = o(4);
            let kind = match get(&o, "kind") {
                Value::String(s) if s == "dot" => Pattern::dot(match get(&o, "density") {
                    Value::Nil => 50.0,
                    v => as_num(&v).filter(|n| n.is_finite()).ok_or_else(|| {
                        format!("pattern: density must be a finite number, got {}", show(&v))
                    })?,
                }),
                Value::String(s) if s == "line" => Pattern::Line,
                Value::String(s) if s == "cross" => Pattern::Cross,
                _ => return Ok(Cmd::Skip),
            };
            Cmd::Pattern {
                x,
                y,
                w,
                h,
                kind,
                ink: ink(&o),
            }
        }
        _ => Cmd::Skip,
    })
}

/// 0.1 `g:image`: an image or layer made by ctx, resolved now (later changes do not alter this frame); anything
/// else draws nothing.
fn image_command(st: &Shared, a: &[Value]) -> Result<Cmd, String> {
    let id = match a.first() {
        Some(Value::Table(t)) => t.raw_get::<Value>("_id").unwrap_or(Value::Nil),
        _ => Value::Nil,
    };
    let id = match id {
        Value::Nil => return Ok(Cmd::Skip),
        Value::Integer(i) => Some(i64::from(i)),
        Value::Number(n) if n.fract() == 0.0 => Some(n as i64),
        _ => None,
    };
    let img = id.and_then(|id| with(st, |s| s.images.get(&id).cloned()));
    let img = img.ok_or("g:image: the image or layer was released")?;
    let [x, y] = nums("image", &a[1..], ["x", "y"])?;
    let o = opts_at(a, 3);
    let w = side("w", &get(&o, "w"), img.w)?;
    let h = side("h", &get(&o, "h"), img.h)?;
    // far-off positions draw nothing; the clamp keeps the paste arithmetic from overflowing
    let at = |v: f64| v.trunc().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i64;
    Ok(Cmd::Image {
        img,
        x: at(x),
        y: at(y),
        w,
        h,
    })
}

/// `w`/`h` as Python `int()`; Pillow cannot scale to 0 or less, and dithering a huge image costs whole rows.
fn side(key: &str, v: &Value, native: u32) -> Result<u32, String> {
    let n = match v {
        Value::Nil => Some(i64::from(native)),
        Value::Boolean(b) => Some(i64::from(*b)),
        Value::String(s) => s.to_string_lossy().trim().parse::<i64>().ok(),
        v => as_num(v)
            .filter(|n| n.is_finite())
            .map(|n| n.trunc() as i64),
    };
    let max = image::MAX_DRAW_SIDE.max(i64::from(native));
    match n {
        Some(n) if (1..=max).contains(&n) => Ok(n as u32),
        _ => Err(format!(
            "image: {key} must be a whole number from 1 to {max}, got {}",
            show(v)
        )),
    }
}
