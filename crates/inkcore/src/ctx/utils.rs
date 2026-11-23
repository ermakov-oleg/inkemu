//! `ctx.json`, `ctx.base64`, `ctx.hex`, `ctx.hash`, `ctx.url`, `ctx.zlib` and `ctx.random` (0.1 `_install_utils`).
//! Strings are bytes end to end: a Lua string never goes through a Rust `String` unless it must be text.
use std::io::Read;

use hmac::{Hmac, Mac};
use md5::Md5;
use mlua::{IntoLuaMulti, Lua, Table, Value};
use rand_chacha::rand_core::RngCore;
use sha1::Sha1;
use sha2::{Digest, Sha256};

use super::convert::{big_int_warning, dump, json_to_lua, lua_to_json};
use super::object::{method, new_object, Method};
use super::{arg, bytes_arg, check_arg, py_int, raw, soft_fail};
use crate::emu::{with, Shared};
use crate::lua::first_in_callback;

/// Largest `n` for `random:bytes` (API 2.33.1 §26).
const MAX_RANDOM_BYTES: i64 = 1024;
/// Input limit of the hashes and encoders (§26).
const MAX_INPUT: usize = 1 << 20;
/// Most items `url:query` takes (§26).
const MAX_QUERY_ITEMS: usize = 64;

const NULL: &str =
    "return setmetatable({}, {__tostring = function() return 'null' end, __metatable = false})";

/// What a codec returns: a Lua string, or `(nil, "invalid_argument")` on Err.
type Codec = fn(&[u8]) -> Result<Vec<u8>, ()>;

fn codec(name: &'static str, limited: bool, f: Codec) -> (&'static str, Method) {
    let m = method(move |lua, args| {
        let b = bytes_arg(name, 1, args.iter().next().unwrap_or(&Value::Nil))?;
        if limited && b.len() > MAX_INPUT {
            return soft_fail(lua, "limit_exceeded");
        }
        match f(&b) {
            Ok(out) => lua.create_string(out)?.into_lua_multi(lua),
            Err(()) => soft_fail(lua, "invalid_argument"),
        }
    });
    (name, m)
}

pub(super) fn install(lua: &Lua, ctx: &Table, st: &Shared) -> mlua::Result<()> {
    let null: Table = lua.load(NULL).set_name("=[host]").eval()?;
    let decode = {
        let (st, null) = (st.clone(), null.clone());
        method(move |lua, args| {
            let text = bytes_arg("decode", 1, args.iter().next().unwrap_or(&Value::Nil))?;
            let Ok(data) = serde_json::from_slice::<serde_json::Value>(&text) else {
                return soft_fail(lua, "invalid_argument");
            };
            let mut big = None;
            let v = json_to_lua(lua, &data, Some(&null), &mut |n| {
                big.get_or_insert_with(|| big_int_warning(n));
            })?;
            if let Some(w) = big.filter(|_| first_in_callback(lua, "json-big-int")) {
                with(&st, |s| s.warn(&w));
            }
            v.into_lua_multi(lua)
        })
    };
    let encode = {
        let (st, null) = (st.clone(), null.clone());
        method(move |lua, args| {
            let v = args.into_iter().next().unwrap_or(Value::Nil);
            if let Value::String(s) = &v {
                if s.to_str().is_err() {
                    return soft_fail(lua, "invalid_argument");
                }
            }
            let mut warnings = vec![];
            let data = lua_to_json(&v, "json", Some(&null), &mut |w| warnings.push(w));
            with(&st, |s| warnings.iter().for_each(|w| s.warn(w)));
            match data {
                Some(d) => dump(&d, true).into_lua_multi(lua),
                None => soft_fail(lua, "invalid_argument"),
            }
        })
    };
    let json = new_object(lua, "json", &[("decode", decode), ("encode", encode)])?;
    json.raw_set("null", null)?;
    ctx.raw_set("json", json)?;

    let crc32 = method(|lua, args| {
        let b = bytes_arg("crc32", 1, args.iter().next().unwrap_or(&Value::Nil))?;
        if b.len() > MAX_INPUT {
            return soft_fail(lua, "limit_exceeded");
        }
        // signed 32-bit integer on the device, values from 2^31 wrap to negative (verified on 26112001)
        Value::Integer(crc32fast::hash(&b) as i32).into_lua_multi(lua)
    });
    let hmac_sha256 = method(|lua, args| {
        let a = args.into_vec();
        let key = bytes_arg("hmac_sha256", 1, a.first().unwrap_or(&Value::Nil))?;
        let msg = bytes_arg("hmac_sha256", 2, a.get(1).unwrap_or(&Value::Nil))?;
        if key.len() > MAX_INPUT || msg.len() > MAX_INPUT {
            return soft_fail(lua, "limit_exceeded");
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("HMAC takes a key of any length");
        mac.update(&msg);
        lua.create_string(mac.finalize().into_bytes())?
            .into_lua_multi(lua)
    });
    let objects: [(&str, Vec<(&str, Method)>); 5] = [
        (
            "base64",
            vec![base64_encode_method(), codec("decode", true, base64_decode)],
        ),
        (
            "hex",
            vec![
                codec("encode", true, |b| Ok(hex(b).into_bytes())),
                codec("decode", true, unhex),
            ],
        ),
        (
            "hash",
            vec![
                codec("sha1", true, |b| Ok(Sha1::digest(b).to_vec())),
                codec("sha256", true, |b| Ok(Sha256::digest(b).to_vec())),
                codec("md5", true, |b| Ok(Md5::digest(b).to_vec())),
                ("crc32", crc32),
                ("hmac_sha256", hmac_sha256),
            ],
        ),
        (
            "url",
            vec![
                codec("encode", true, |b| Ok(url_encode(b).into_bytes())),
                codec("decode", true, url_decode),
                ("query", url_query()),
            ],
        ),
        ("zlib", vec![codec("decompress", false, decompress)]),
    ];
    for (name, methods) in objects {
        ctx.raw_set(name, new_object(lua, name, &methods)?)?;
    }

    let uuid = {
        let st = st.clone();
        method(move |lua, _| {
            let mut b = [0u8; 16];
            with(&st, |s| s.rng.fill_bytes(&mut b));
            uuid4(b).into_lua_multi(lua)
        })
    };
    let bytes = {
        let st = st.clone();
        method(move |lua, args| {
            let a = args.into_vec();
            check_arg("bytes", 1, arg(&a, 0), "number")?;
            let n = py_int(arg(&a, 0))?;
            if !(1..=MAX_RANDOM_BYTES).contains(&n) {
                return soft_fail(lua, "invalid_argument");
            }
            let mut b = vec![0u8; n as usize];
            with(&st, |s| s.rng.fill_bytes(&mut b));
            lua.create_string(b)?.into_lua_multi(lua)
        })
    };
    ctx.raw_set(
        "random",
        new_object(lua, "random", &[("bytes", bytes), ("uuid", uuid)])?,
    )
}

/// `base64:encode(s, opts)`: `opts.url = true` selects the `-_` alphabet without padding.
fn base64_encode_method() -> (&'static str, Method) {
    let m = method(|lua, args| {
        let a = args.into_vec();
        let b = bytes_arg("encode", 1, arg(&a, 0))?;
        if !matches!(arg(&a, 1), Value::Nil) {
            check_arg("encode", 2, arg(&a, 1), "table")?;
        }
        let url = match arg(&a, 1) {
            Value::Table(t) => match t.get::<Value>("url")? {
                Value::Nil => false,
                Value::Boolean(b) => b,
                _ => return soft_fail(lua, "invalid_argument"),
            },
            _ => false,
        };
        if b.len() > MAX_INPUT {
            return soft_fail(lua, "limit_exceeded");
        }
        let mut out = base64_encode(&b);
        if url {
            out = out.replace('+', "-").replace('/', "_").replace('=', "");
        }
        lua.create_string(out)?.into_lua_multi(lua)
    });
    ("encode", m)
}

/// `url:query(t)`: `key=value` items in byte order of the keys, both percent-encoded.
fn url_query() -> Method {
    method(|lua, args| {
        let a = args.into_vec();
        check_arg("query", 1, arg(&a, 0), "table")?;
        let Value::Table(t) = arg(&a, 0) else {
            unreachable!("checked above")
        };
        let mut items = vec![];
        for pair in t.pairs::<Value, Value>() {
            let (k, v) = pair?;
            let (Value::String(k), Some(v)) = (&k, query_value(&v)) else {
                return soft_fail(lua, "invalid_argument");
            };
            items.push((k.as_bytes().to_vec(), v));
        }
        if items.len() > MAX_QUERY_ITEMS {
            return soft_fail(lua, "limit_exceeded");
        }
        items.sort();
        let out: Vec<String> = items
            .iter()
            .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
            .collect();
        lua.create_string(out.join("&"))?.into_lua_multi(lua)
    })
}

fn query_value(v: &Value) -> Option<Vec<u8>> {
    match v {
        Value::Boolean(b) => Some(b.to_string().into_bytes()),
        Value::String(_) | Value::Integer(_) | Value::Number(_) => raw(v),
        _ => None,
    }
}

/// Python `str(uuid.UUID(int=..., version=4))`.
fn uuid4(mut b: [u8; 16]) -> String {
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = hex(&b);
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

pub(super) fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

fn unhex(b: &[u8]) -> Result<Vec<u8>, ()> {
    if !b.len().is_multiple_of(2) || !b.iter().all(u8::is_ascii_hexdigit) {
        return Err(());
    }
    b.chunks(2)
        .map(|p| {
            let s = std::str::from_utf8(p).map_err(|_| ())?;
            u8::from_str_radix(s, 16).map_err(|_| ())
        })
        .collect()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for chunk in b.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &c)| n | u32::from(c) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(B64[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Both alphabets (`+/` and `-_`); `=` padding is optional but, when present, must complete the last group.
fn base64_decode(b: &[u8]) -> Result<Vec<u8>, ()> {
    let data_len = b.iter().position(|&c| c == b'=').unwrap_or(b.len());
    let (data, pads) = b.split_at(data_len);
    if pads.len() > 2 || pads.iter().any(|&c| c != b'=') {
        return Err(());
    }
    let mut vals = Vec::with_capacity(data.len());
    for &c in data {
        let v = match c {
            b'-' => 62,
            b'_' => 63,
            _ => B64.iter().position(|&a| a == c).ok_or(())?,
        };
        vals.push(v as u32);
    }
    let want_pads = match data.len() % 4 {
        0 => 0,
        2 => 2,
        3 => 1,
        _ => return Err(()),
    };
    if !pads.is_empty() && pads.len() != want_pads {
        return Err(());
    }
    let mut out = Vec::with_capacity(data.len() / 4 * 3 + 2);
    for group in vals.chunks(4) {
        let n = group
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &v)| n | v << (18 - 6 * i));
        out.extend_from_slice(&n.to_be_bytes()[1..group.len()]);
    }
    Ok(out)
}

/// Python `urllib.parse.quote(b, safe="-_.~")`.
fn url_encode(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len());
    for &c in b {
        if c.is_ascii_alphanumeric() || b"-_.~".contains(&c) {
            out.push(char::from(c));
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

/// Only `%XX` is decoded (`+` stays); a malformed escape is an error.
fn url_decode(b: &[u8]) -> Result<Vec<u8>, ()> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let p = b.get(i + 1..i + 3).ok_or(())?;
            if !p.iter().all(u8::is_ascii_hexdigit) {
                return Err(());
            }
            let s = std::str::from_utf8(p).map_err(|_| ())?;
            out.push(u8::from_str_radix(s, 16).map_err(|_| ())?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Ok(out)
}

/// Python `zlib.decompress(b, 47)` (zlib or gzip header), falling back to raw deflate.
fn decompress(b: &[u8]) -> Result<Vec<u8>, ()> {
    use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
    let run = |mut r: Box<dyn Read + '_>| {
        let mut out = vec![];
        r.read_to_end(&mut out).ok().map(|_| out)
    };
    let headered: Box<dyn Read> = if b.starts_with(&[0x1f, 0x8b]) {
        Box::new(GzDecoder::new(b))
    } else {
        Box::new(ZlibDecoder::new(b))
    };
    run(headered)
        .or_else(|| run(Box::new(DeflateDecoder::new(b))))
        .ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_as_python() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"A"), "QQ==");
        assert_eq!(base64_encode(b"AB"), "QUI=");
        assert_eq!(base64_encode(b"ABC"), "QUJD");
        assert_eq!(base64_decode(b"QQ==").unwrap(), b"A");
        assert_eq!(base64_decode(b"QUI=").unwrap(), b"AB");
        assert_eq!(base64_decode(b"QUJD").unwrap(), b"ABC");
        assert_eq!(base64_decode(b"QR==").unwrap(), b"A");
        assert_eq!(base64_decode(b"QQ").unwrap(), b"A");
        assert_eq!(base64_decode(b"QUI").unwrap(), b"AB");
        assert_eq!(base64_decode(b"-_-_").unwrap(), b"\xfb\xff\xbf");
        assert_eq!(base64_decode(b"").unwrap(), b"");
        for bad in [
            &b"QQ="[..],
            b"QUI==",
            b"Q",
            b"Q===",
            b"QUJDQ",
            b"QUJD=",
            b"QUJD==",
            b"=",
            b"==",
            b"***",
            b"QQ=A",
            b"QQ===",
        ] {
            assert!(base64_decode(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn url_and_hex_as_python() {
        assert_eq!(url_encode("a b/я?~".as_bytes()), "a%20b%2F%D1%8F%3F~");
        assert_eq!(url_decode(b"a%20b%2f+").unwrap(), b"a b/+");
        assert!(
            url_decode(b"%zz").is_err() && url_decode(b"%4").is_err() && url_decode(b"%").is_err()
        );
        assert_eq!(unhex(b"00Ff61").unwrap(), b"\0\xffa");
        assert!(unhex(b"abc").is_err() && unhex(b"zz").is_err() && unhex(b"+1").is_err());
    }

    #[test]
    fn truncated_zlib_is_an_error() {
        let z = [
            0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x06, 0x2c, 0x02, 0x15,
        ];
        assert_eq!(decompress(&z).unwrap(), b"hello");
        assert!(decompress(&z[..8]).is_err());
        assert!(decompress(b"nope").is_err());
        assert!(decompress(b"").is_err());
    }
}
