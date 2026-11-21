//! Images for `g:image`: 8-bit gray as 0.1 kept them (Pillow mode "L"), scaled, dithered and pasted onto the 1-bit
//! frame as Pillow did (`resize` NEAREST/LANCZOS, `convert("1")` Floyd–Steinberg, `paste`).
use super::Frame;

/// The largest `w`/`h` an image is drawn at: dithering needs whole rows of the scaled image.
pub const MAX_DRAW_SIDE: i64 = 8192;

/// 0 black … 255 white, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct Gray {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

pub enum Decoded {
    Image { img: Gray, png: bool },
    Failed,
}

/// PNG or JPEG bytes as 0.1 `decode` + `_flatten`: gray, transparency composited over white.
pub fn decode(data: &[u8]) -> Decoded {
    let png = image::guess_format(data).is_ok_and(|f| f == image::ImageFormat::Png);
    let Ok(img) = image::load_from_memory(data) else {
        return Decoded::Failed;
    };
    let alpha = img.color().has_alpha();
    let rgba = img.to_rgba8();
    let px = rgba
        .pixels()
        .map(|p| {
            let [r, g, b, a] = p.0;
            if alpha {
                luma(over_white(r, a), over_white(g, a), over_white(b, a))
            } else {
                luma(r, g, b)
            }
        })
        .collect();
    Decoded::Image {
        img: Gray {
            w: rgba.width(),
            h: rgba.height(),
            px,
        },
        png,
    }
}

/// Pillow's `L24` RGB to gray.
fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) * 19595 + u32::from(g) * 38470 + u32::from(b) * 7471 + 0x8000) >> 16) as u8
}

/// Pillow's `alpha_composite` of one channel over opaque white.
fn over_white(c: u8, a: u8) -> u8 {
    if a == 0 {
        return 255;
    }
    let (c, a) = (u32::from(c), u32::from(a));
    let tmp = (c * a + 255 * (255 - a)) * 128 + (0x80 << 7);
    ((((tmp >> 8) + tmp) >> 8) >> 7) as u8
}

impl Gray {
    pub fn white(w: u32, h: u32) -> Self {
        Gray {
            w,
            h,
            px: vec![255; w as usize * h as usize],
        }
    }

    /// A 1-bit frame of this image (only ever 0 or 255 for layers, so no dithering).
    pub fn to_frame(&self) -> Frame {
        Frame {
            w: self.w,
            h: self.h,
            px: self.px.iter().map(|&v| u8::from(v < 128)).collect(),
        }
    }

    pub fn from_frame(f: &Frame) -> Self {
        Gray {
            w: f.w,
            h: f.h,
            px: f.px.iter().map(|&v| if v == 1 { 0 } else { 255 }).collect(),
        }
    }
}

/// 0.1 `g:image`: `img` scaled to `w`×`h` (1..=MAX_DRAW_SIDE), dithered, pasted with its top left at `x`, `y`.
pub fn paste(f: &mut Frame, img: &Gray, x: i64, y: i64, w: u32, h: u32) {
    let rows = (i64::from(f.h) - y).clamp(0, i64::from(h)) as usize;
    let visible_x = x < i64::from(f.w) && x + i64::from(w) > 0;
    if rows == 0 || y + i64::from(h) <= 0 || !visible_x {
        return;
    }
    let scaled = if (w, h) == (img.w, img.h) {
        img.px[..rows * w as usize].to_vec()
    } else if w >= img.w {
        nearest(img, w as usize, h as usize, rows)
    } else {
        lanczos(img, w as usize, h as usize, rows)
    };
    let bits = dither(&scaled, w as usize, rows);
    for (r, row) in bits.chunks(w as usize).enumerate() {
        let py = y + r as i64;
        if py < 0 {
            continue;
        }
        for (c, &v) in row.iter().enumerate() {
            let px = x + c as i64;
            if (0..i64::from(f.w)).contains(&px) {
                f.px[(py as usize) * f.w as usize + px as usize] = u8::from(v == 0);
            }
        }
    }
}

/// Pillow's `ImagingScaleAffine` source positions: accumulated in f64 as Pillow does, so ties fall the same way.
fn nearest_index(src: usize, dst: usize, n: usize) -> Vec<usize> {
    let step = src as f64 / dst as f64;
    let mut at = step * 0.5;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push((at as usize).min(src - 1));
        at += step;
    }
    out
}

fn nearest(img: &Gray, w: usize, h: usize, rows: usize) -> Vec<u8> {
    let xs = nearest_index(img.w as usize, w, w);
    let ys = nearest_index(img.h as usize, h, rows);
    let mut out = Vec::with_capacity(w * rows);
    for sy in ys {
        let line = &img.px[sy * img.w as usize..][..img.w as usize];
        out.extend(xs.iter().map(|&sx| line[sx]));
    }
    out
}

const PRECISION_BITS: u32 = 32 - 8 - 2;

fn lanczos_filter(x: f64) -> f64 {
    fn sinc(x: f64) -> f64 {
        if x == 0.0 {
            return 1.0;
        }
        let x = x * std::f64::consts::PI;
        x.sin() / x
    }
    if (-3.0..3.0).contains(&x) {
        sinc(x) * sinc(x / 3.0)
    } else {
        0.0
    }
}

/// Pillow's `precompute_coeffs` + `normalize_coeffs_8bpc` for the first `n` outputs: (first source index, weights).
fn coeffs(src: usize, dst: usize, n: usize) -> Vec<(usize, Vec<i64>)> {
    let scale = src as f64 / dst as f64;
    let filterscale = scale.max(1.0);
    let support = 3.0 * filterscale;
    (0..n)
        .map(|xx| {
            let center = (xx as f64 + 0.5) * scale;
            let xmin = ((center - support + 0.5) as i64).max(0) as usize;
            let xmax = ((center + support + 0.5) as i64).min(src as i64) as usize - xmin;
            let w: Vec<f64> = (0..xmax)
                .map(|x| lanczos_filter((x as f64 + xmin as f64 - center + 0.5) / filterscale))
                .collect();
            let total: f64 = w.iter().sum();
            let k = w
                .iter()
                .map(|&v| {
                    let v = if total != 0.0 { v / total } else { v };
                    let f = v * f64::from(1u32 << PRECISION_BITS);
                    (if v < 0.0 { f - 0.5 } else { f + 0.5 }) as i64
                })
                .collect();
            (xmin, k)
        })
        .collect()
}

fn clip8(v: i64) -> u8 {
    if v >= 1 << PRECISION_BITS << 8 {
        255
    } else if v <= 0 {
        0
    } else {
        (v >> PRECISION_BITS) as u8
    }
}

/// One separable pass: `get(i)` reads the i-th source sample along the axis.
fn apply(k: &(usize, Vec<i64>), get: impl Fn(usize) -> u8) -> u8 {
    let (first, w) = k;
    let sum = w
        .iter()
        .enumerate()
        .fold(1i64 << (PRECISION_BITS - 1), |s, (i, &kv)| {
            s + i64::from(get(first + i)) * kv
        });
    clip8(sum)
}

/// Pillow's LANCZOS `resize` (horizontal then vertical, unless height shrinks much more), first `rows` rows only.
fn lanczos(img: &Gray, w: usize, h: usize, rows: usize) -> Vec<u8> {
    let (sw, sh) = (img.w as usize, img.h as usize);
    let hk = coeffs(sw, w, w);
    let src = |x: usize, y: usize| img.px[y * sw + x];
    if h == sh {
        return (0..rows)
            .flat_map(|y| hk.iter().map(move |k| (k, y)))
            .map(|(k, y)| apply(k, |x| src(x, y)))
            .collect();
    }
    let vk = coeffs(sh, h, rows);
    let horizontal_first = !(sh > h && (sh - h) as i64 > (sw as i64 - w as i64) * 2);
    if horizontal_first {
        let first = vk[0].0;
        let last = vk.iter().map(|(y, k)| y + k.len()).max().unwrap_or(first);
        let tmp: Vec<u8> = (first..last)
            .flat_map(|y| hk.iter().map(move |k| (k, y)))
            .map(|(k, y)| apply(k, |x| src(x, y)))
            .collect();
        (0..rows)
            .flat_map(|r| (0..w).map(move |x| (r, x)))
            .map(|(r, x)| apply(&vk[r], |y| tmp[(y - first) * w + x]))
            .collect()
    } else {
        let tmp: Vec<u8> = (0..rows)
            .flat_map(|r| (0..sw).map(move |x| (r, x)))
            .map(|(r, x)| apply(&vk[r], |y| src(x, y)))
            .collect();
        (0..rows)
            .flat_map(|r| hk.iter().map(move |k| (k, r)))
            .map(|(k, r)| apply(k, |x| tmp[r * sw + x]))
            .collect()
    }
}

/// Pillow's `convert("1")`: Floyd–Steinberg error diffusion to 0 or 255, integer arithmetic as in `tobilevel`.
fn dither(px: &[u8], w: usize, rows: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * rows];
    let mut errors = vec![0i32; w + 1];
    for y in 0..rows {
        let (mut l, mut l0, mut l1) = (0i32, 0i32, 0i32);
        for x in 0..w {
            l = (i32::from(px[y * w + x]) + (l + errors[x + 1]) / 16).clamp(0, 255);
            let o = if l > 128 { 255 } else { 0 };
            out[y * w + x] = o as u8;
            l -= o;
            let l2 = l;
            let d2 = l + l;
            l += d2;
            errors[x] = l + l0;
            l += d2;
            l0 = l + l1;
            l1 = l2;
            l += d2;
        }
        errors[w] = l0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(w: u32, h: u32, f: impl Fn(u32, u32) -> u8) -> Gray {
        Gray {
            w,
            h,
            px: (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .map(|(x, y)| f(x, y))
                .collect(),
        }
    }

    // Expected values from Pillow 13 on the same images (`convert("1")`, `resize(..., LANCZOS/NEAREST)`).
    #[test]
    fn dither_and_resize_match_pillow() {
        let img = gray(5, 3, |x, y| ((y * 5 + x) * 17) as u8);
        assert_eq!(
            dither(&img.px, 5, 3),
            [0, 0, 0, 0, 0, 0, 255, 0, 255, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(lanczos(&img, 3, 2, 2), [28, 57, 86, 152, 181, 210]);
        assert_eq!(lanczos(&img, 4, 1, 1), [87, 108, 130, 151]);
        assert_eq!(
            lanczos(&img, 3, 6, 6),
            [0, 26, 55, 15, 44, 73, 57, 86, 115, 123, 152, 181, 165, 194, 223, 183, 212, 241]
        );
        assert_eq!(nearest(&img, 7, 4, 4)[..7], [0, 17, 17, 34, 51, 51, 68]);
        // height shrinks far more than width: Pillow runs the vertical pass first
        let tall = gray(6, 40, |x, y| ((y * 6 + x) * 37 % 256) as u8);
        assert_eq!(
            lanczos(&tall, 5, 3, 3),
            [124, 122, 122, 125, 132, 121, 128, 132, 135, 130, 125, 122, 124, 135, 126]
        );
    }

    #[test]
    fn huge_sizes_only_compute_the_rows_on_screen() {
        let img = gray(2, 2, |x, _| if x == 0 { 0 } else { 255 });
        let mut f = Frame::new(10, 10);
        paste(&mut f, &img, -4, -4, 8192, 8192);
        assert!(f.px.iter().all(|&v| v == 1));
    }
}
