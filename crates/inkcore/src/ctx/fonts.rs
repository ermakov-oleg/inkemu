//! `ctx.fonts` (0.1 `_install_fonts`, API 2.33 §7).
use mlua::{IntoLuaMulti, Lua, Table, Value};

use super::object::{method, new_object};
use super::{arg, check_arg, fmt, int_value, soft_fail, type_name};
use crate::draw::text::FONT_NAMES;
use crate::draw::{has_rtl, layout_opts, utf8_text, CALL_TEXT_BYTES};
use crate::emu::{with, Shared};

/// The bytes one `font:layout` call takes.
const LAYOUT_BYTES: usize = 65536;
/// The longest `sample` of `ctx.fonts:size`.
const SAMPLE_BYTES: usize = 256;

fn opts(a: &[Value], i: usize) -> Option<Table> {
    match a.get(i) {
        Some(Value::Table(t)) => Some(t.clone()),
        _ => None,
    }
}

/// The text of a `measure`/`layout` argument (strings as they are, anything else as 0.1 printed it), or the
/// refusal code: too long, not UTF-8, or right-to-left.
fn text(v: &Value, max: usize) -> Result<String, &'static str> {
    let bytes = match v {
        Value::String(s) => s.as_bytes().to_vec(),
        v => fmt(v).into_bytes(),
    };
    let s = utf8_text(bytes, max)?;
    if has_rtl(&s) {
        return Err("invalid_argument");
    }
    Ok(s)
}

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let fonts = with(st, |s| s.fonts.clone());
    let mut handles = vec![];
    for name in FONT_NAMES {
        let (f, st1) = (fonts.clone(), st.clone());
        let measure = method(move |lua, args| {
            let a = args.into_vec();
            let s = match text(arg(&a, 0), CALL_TEXT_BYTES) {
                Ok(s) => s,
                Err(code) => return soft_fail(lua, code),
            };
            let o = layout_opts(&opts(&a, 1), with(&st1, |s| s.app_api));
            f.measure_with(name, &s, o.style).into_lua_multi(lua)
        });
        let (f, st1) = (fonts.clone(), st.clone());
        let layout = method(move |lua, args| {
            let a = args.into_vec();
            check_arg("layout", 2, arg(&a, 1), "number")?;
            check_arg("layout", 3, arg(&a, 2), "number")?;
            let s = match text(arg(&a, 0), LAYOUT_BYTES) {
                Ok(s) => s,
                Err(code) => return soft_fail(lua, code),
            };
            let num = |v: &Value| match *v {
                Value::Integer(i) => f64::from(i),
                Value::Number(n) => f64::from(n),
                _ => 0.0,
            };
            let o = layout_opts(&opts(&a, 3), with(&st1, |s| s.app_api));
            let lay = f.layout(name, &s, num(arg(&a, 1)), num(arg(&a, 2)), &o);
            let lines = i64::try_from(lay.lines.len()).unwrap_or(i64::MAX);
            let consumed = i64::try_from(lay.consumed).unwrap_or(i64::MAX);
            (
                int_value(lines),
                int_value(i64::from(lay.height)),
                int_value(consumed),
            )
                .into_lua_multi(lua)
        });
        let h = new_object(
            lua,
            &format!("fonts.{name}"),
            &[("measure", measure), ("layout", layout)],
        )?;
        h.raw_set("name", name)?;
        h.raw_set("line_height", fonts.line_height(name))?;
        h.raw_set("ascent", fonts.ascent(name))?;
        handles.push((name, h));
    }
    let size = {
        let handles = handles.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            let px = match *arg(&a, 0) {
                Value::Integer(i) => Some(f64::from(i)),
                Value::Number(n) => Some(f64::from(n)),
                _ => None,
            };
            let Some(px) = px.filter(|p| p.fract() == 0.0 && (8.0..=96.0).contains(p)) else {
                return soft_fail(lua, "invalid_argument");
            };
            match arg(&a, 1) {
                Value::Nil => {}
                Value::String(s) => {
                    let ok = utf8_text(s.as_bytes().to_vec(), SAMPLE_BYTES)
                        .is_ok_and(|s| !s.contains('\0') && !has_rtl(&s));
                    if !ok {
                        return soft_fail(lua, "invalid_argument");
                    }
                }
                v => {
                    return Err(mlua::Error::runtime(format!(
                        "bad argument #2 to 'size' (string expected, got {})",
                        type_name(v)
                    )))
                }
            }
            // the first of equally near sizes wins, as Python's min()
            let mut best = &handles[0];
            for h in &handles[1..] {
                let d = |name| (f64::from(fonts.line_height(name)) - px).abs();
                if d(h.0) < d(best.0) {
                    best = h;
                }
            }
            best.1.clone().into_lua_multi(lua)
        })
    };
    let table = new_object(lua, "fonts", &[("size", size)])?;
    for (name, h) in handles {
        table.raw_set(name, h)?;
    }
    ctx.raw_set("fonts", table)
}
