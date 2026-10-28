//! OKLab / OKLCH and sRGB conversion, gamut mapping, hex output.

use std::f64::consts::PI;

pub type Lab = [f64; 3];

pub const DEG: f64 = PI / 180.0;

/// sRGB byte -> linear, as a table: the downsampled image is u8 per channel.
pub fn lin_table() -> [f64; 256] {
    let mut t = [0.0; 256];
    for (i, v) in t.iter_mut().enumerate() {
        let c = i as f64 / 255.0;
        *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    }
    t
}

pub fn linear_to_oklab([r, g, b]: [f64; 3]) -> Lab {
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn oklab_to_linear([ll, a, b]: Lab) -> [f64; 3] {
    let l = (ll + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (ll - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (ll - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034133289 * m + 1.7076127010 * s,
    ]
}

fn in_gamut(lab: Lab) -> bool {
    oklab_to_linear(lab).iter().all(|&c| (-1e-4..=1.0 + 1e-4).contains(&c))
}

/// [L, C, h] of an OKLab colour.
pub fn lch([l, a, b]: Lab) -> [f64; 3] {
    [l, a.hypot(b), b.atan2(a)]
}

pub fn from_lch(l: f64, c: f64, h: f64) -> Lab {
    [l, c * h.cos(), c * h.sin()]
}

/// Unsigned angle between two hues, in [0, PI].
pub fn hue_gap(a: f64, b: f64) -> f64 {
    ((a - b + PI).rem_euclid(2.0 * PI) - PI).abs()
}

pub fn dist(a: Lab, b: Lab) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// `#RRGGBB`, out-of-gamut colours brought in by shrinking chroma at fixed
/// lightness and hue.
pub fn hex(lab: Lab) -> String {
    let mut lab = lab;
    if !in_gamut(lab) {
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if in_gamut([lab[0], lab[1] * mid, lab[2] * mid]) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lab = [lab[0], lab[1] * lo, lab[2] * lo];
    }
    let enc = |c: f64| {
        let c = c.clamp(0.0, 1.0);
        let v = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
        (v * 255.0).round() as u8
    };
    let [r, g, b] = oklab_to_linear(lab);
    format!("#{:02X}{:02X}{:02X}", enc(r), enc(g), enc(b))
}
