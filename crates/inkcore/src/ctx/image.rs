//! `ctx.image` and `ctx.qr` (0.1 `ctx_ui.decode`, `qr_encode`, `new_image`).
use std::rc::Rc;

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value};
use qrcode::bits::Bits;
use qrcode::canvas::Canvas;
use qrcode::{Color, EcLevel, Version};

use super::object::{method, new_object, Method};
use super::{bytes_arg, soft_fail};
use crate::draw::image::{decode, Decoded, Gray};
use crate::emu::{with, Shared};

/// Whether `release` frees an image: package images stay drawable after it (§8), the others do not.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Package,
    Runtime,
}

/// An image object (`width`, `height`, `_id`, `release` plus `methods`) for a new entry in the image table.
pub(super) fn new_image(
    lua: &Lua,
    st: &Shared,
    img: Gray,
    kind: Kind,
    name: &str,
    methods: Vec<(&str, Method)>,
) -> mlua::Result<(Table, i64)> {
    let (w, h) = (img.w, img.h);
    let id = with(st, |s| {
        s.next_image += 1;
        s.images.insert(s.next_image, Rc::new(img));
        s.next_image
    });
    let release = {
        let st = st.clone();
        method(move |lua, _| {
            // the device unloads a package image and decodes it again when drawn: invisible to the script
            if kind == Kind::Runtime {
                with(&st, |s| s.images.remove(&id));
            }
            true.into_lua_multi(lua)
        })
    };
    let mut all = vec![("release", release)];
    all.extend(methods);
    let obj = new_object(lua, name, &all)?;
    obj.raw_set("width", w)?;
    obj.raw_set("height", h)?;
    obj.raw_set("_id", id)?;
    Ok((obj, id))
}

/// `image:decode(data)` and `package:image(name)`: the image, or `nil, "decode_failed"`.
pub(super) fn decode_image(
    lua: &Lua,
    st: &Shared,
    data: &[u8],
    kind: Kind,
) -> mlua::Result<MultiValue> {
    match decode(data) {
        Decoded::Failed => soft_fail(lua, "decode_failed"),
        Decoded::Image { img, png } => {
            if png {
                with(st, |s| {
                    s.warn("image:decode: PNG may not decode on the device (older firmware closes the app); use JPEG")
                });
            }
            new_image(lua, st, img, kind, "image", vec![])?
                .0
                .into_lua_multi(lua)
        }
    }
}

/// segno's `make_qr(text, error="m", boost_error=False)`: one segment in the densest mode that fits the whole text,
/// text as Latin-1 when it can be, else UTF-8, the smallest version. Kanji mode is not used.
fn qr(text: &str) -> Option<Gray> {
    const ALNUM: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";
    let latin1: Option<Vec<u8>> = text.chars().map(|c| u8::try_from(c).ok()).collect();
    let data = latin1.unwrap_or_else(|| text.as_bytes().to_vec());
    let (version, codewords) = (1..=40).find_map(|v| {
        let mut bits = Bits::new(Version::Normal(v));
        let pushed = if !data.is_empty() && data.iter().all(u8::is_ascii_digit) {
            bits.push_numeric_data(&data)
        } else if !data.is_empty() && data.iter().all(|b| ALNUM.contains(b)) {
            bits.push_alphanumeric_data(&data)
        } else {
            bits.push_byte_data(&data)
        };
        pushed.ok()?;
        let (len, cap) = (bits.len(), bits.max_len(EcLevel::M).ok()?);
        if len > cap {
            return None;
        }
        // segno pads with 8 zero bits even when the terminator already ends on a byte boundary (its
        // write_padding_bits); qrcode's push_terminator does not, so the data codewords are built here
        let ended = len + (cap - len).min(4);
        let n = (ended + 8 - ended % 8) / 8;
        if n > cap / 8 {
            return None;
        }
        let mut cw = bits.into_bytes();
        cw.resize(n, 0);
        cw.extend([0xec, 0x11].iter().cycle().take(cap / 8 - n));
        Some((Version::Normal(v), cw))
    })?;
    let (data_cw, ec_cw) = qrcode::ec::construct_codewords(&codewords, version, EcLevel::M).ok()?;
    let mut canvas = Canvas::new(version, EcLevel::M);
    canvas.draw_all_functional_patterns();
    canvas.draw_data(&data_cw, &ec_cw);
    let side = version.width() as u32;
    let px = canvas
        .apply_best_mask()
        .into_colors()
        .into_iter()
        .map(|c| if c == Color::Dark { 0 } else { 255 })
        .collect();
    Some(Gray {
        w: side,
        h: side,
        px,
    })
}

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let decode = {
        let st = st.clone();
        method(move |lua, args| {
            let data = bytes_arg("decode", 1, args.iter().next().unwrap_or(&Value::Nil))?;
            decode_image(lua, &st, &data, Kind::Runtime)
        })
    };
    ctx.raw_set("image", new_object(lua, "image", &[("decode", decode)])?)?;
    let encode = {
        let st = st.clone();
        method(move |lua, args| {
            let data = bytes_arg("encode", 1, args.iter().next().unwrap_or(&Value::Nil))?;
            match std::str::from_utf8(&data).ok().and_then(qr) {
                Some(img) => new_image(lua, &st, img, Kind::Runtime, "image", vec![])?
                    .0
                    .into_lua_multi(lua),
                None => soft_fail(lua, "invalid_argument"),
            }
        })
    };
    ctx.raw_set("qr", new_object(lua, "qr", &[("encode", encode)])?)
}

#[cfg(test)]
mod tests {
    use super::qr;

    #[test]
    fn qr_matches_segno_including_its_extra_pad_byte() {
        // segno.make_qr('hi', error='m', boost_error=False).matrix
        let segno = [
            "#######.#.##..#######",
            "#.....#..##.#.#.....#",
            "#.###.#..###..#.###.#",
            "#.###.#.####..#.###.#",
            "#.###.#.#.#.#.#.###.#",
            "#.....#.####..#.....#",
            "#######.#.#.#.#######",
            "........#.###........",
            "#...#.####.#.#####..#",
            ".#.#.#.###.##..#...#.",
            ".####.###..#..###..#.",
            "#.#.#..#..#..##.##...",
            "###.#####.#.###...##.",
            "........#.#.###...##.",
            "#######.##..##...#.#.",
            "#.....#...###..#.#..#",
            "#.###.#.#..#..###.###",
            "#.###.#..#.##..#.#.##",
            "#.###.#...##..#####..",
            "#.....#..##..##.#.#..",
            "#######.##..###...#.#",
        ];
        let img = qr("hi").unwrap();
        let ours: Vec<String> = img
            .px
            .chunks(21)
            .map(|r| r.iter().map(|&v| if v == 0 { '#' } else { '.' }).collect())
            .collect();
        assert_eq!(ours, segno);
        assert!(qr(&"x".repeat(5000)).is_none());
    }
}
