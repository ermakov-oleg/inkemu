//! The screen: draw commands, 1-bit rendering, text and the host's overlay plates.
pub mod canvas;
mod pattern;
pub(crate) mod plates;
pub mod raster;
pub mod text;

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value};

pub use canvas::{Align, Canvas, Cmd, TextOpts, CAP};
pub use raster::Style;

use crate::ctx::object::{method, new_object, Method};
use crate::emu::{with, Shared};
use crate::log::py_repr_str;

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

    /// Sets one pixel; outside the frame does nothing.
    pub fn set(&mut self, x: i32, y: i32, v: u8) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            self.px[(y as u32 * self.w + x as u32) as usize] = v;
        }
    }
}

/// Argument kinds of the recorded `g` methods, checked as 0.1 did: n number, t table, s string or number,
/// a any value.
const SPECS: [(&str, &str); 13] = [
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
];

/// The `g` passed to `on_draw`.
pub(crate) fn new_g(lua: &Lua, st: &Shared) -> mlua::Result<Table> {
    let mut methods: Vec<(&str, Method)> = vec![];
    for (name, spec) in SPECS {
        let st = st.clone();
        methods.push((
            name,
            method(move |lua, args| {
                let a = args.into_vec();
                check(name, spec, &a)?;
                let cmd = command(lua, name, &a);
                with(&st, |s| s.canvas.record(cmd));
                Ok(MultiValue::new())
            }),
        ));
    }
    let size = {
        let st = st.clone();
        method(move |lua, _| with(&st, |s| (s.canvas.w, s.canvas.h)).into_lua_multi(lua))
    };
    let measure = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            let s = match a.first() {
                Some(v) => text_of(lua, v),
                None => "nil".into(),
            };
            let font = font_of(&opts_at(&a, 1));
            let fonts = with(&st, |s| s.canvas.fonts());
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

/// Python truthiness of the value 0.1 saw: 0, "" and {} are false.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Nil | Value::Boolean(false) | Value::Integer(0) => false,
        Value::Number(n) => *n != 0.0,
        Value::String(s) => !s.as_bytes().is_empty(),
        Value::Table(t) => !t.is_empty(),
        _ => true,
    }
}

/// 1 black, 0 white: `color=0` (Python `== 0`) is white.
fn ink(o: &Option<Table>) -> u8 {
    match get(o, "color") {
        Value::Integer(0) | Value::Boolean(false) => 0,
        Value::Number(0.0) => 0,
        _ => 1,
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

fn text_opts(o: &Option<Table>) -> TextOpts {
    let pick = |key: &str, mid: &str, end: &str| match get(o, key) {
        Value::String(s) if s == mid => Align::Middle,
        Value::String(s) if s == end => Align::End,
        _ => Align::Start,
    };
    TextOpts {
        font: font_of(o),
        align: pick("align", "center", "right"),
        valign: pick("valign", "middle", "bottom"),
    }
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

/// The checked command for one `g` call; `Err` is an option value the device cannot draw.
fn command(lua: &Lua, op: &str, a: &[Value]) -> Result<Cmd, String> {
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
                text: text_of(lua, &a[2]),
                opts: text_opts(&o(3)),
            }
        }
        "text_box" => {
            let [x, y, w, h] = nums(op, a, ["x", "y", "w", "h"])?;
            Cmd::TextBox {
                x,
                y,
                w,
                h,
                text: text_of(lua, &a[4]),
                opts: text_opts(&o(5)),
            }
        }
        "pattern_rect" | "fill_pattern" => {
            let [x, y, w, h] = nums(op, a, ["x", "y", "w", "h"])?;
            let [x, y, w, h] = [x, y, w, h].map(|v| v.trunc() as i32);
            Cmd::Pattern { x, y, w, h }
        }
        _ => Cmd::Image,
    })
}
