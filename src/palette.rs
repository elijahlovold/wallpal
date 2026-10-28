//! Regions -> palette: a ramp of the image's hue families (color1-4 /
//! 9-12) plus two accents taken from real regions (color5-6 / 13-14), the
//! layout wallust's saliencedark palettes have, so color13 stays the accent.

use std::f64::consts::PI;

use crate::color::{dist, from_lch, hue_gap, lch, Lab, DEG};
use crate::image::Grid;

const K: usize = 12;
/// Regions smaller than this share of the image can't be accents.
const MIN_AREA: f64 = 0.012;
/// Area-weighted chroma under this: treat the image as grey.
const ACHROMATIC: f64 = 0.018;
/// Smallest OKLab distance allowed between two of color1-6.
const MIN_DE: f64 = 0.065;
const RAMP_L: [f64; 4] = [0.50, 0.56, 0.62, 0.67];
const ACCENT_L: f64 = 0.72;
const BRIGHT_DL: f64 = 0.15;
const BG_L: (f64, f64) = (0.08, 0.15);
const BG_L_SCALE: f64 = 0.75;
/// Width of the edge strip --side looks at, as a share of the image.
const EDGE: f64 = 0.2;

/// Which edge of the wallpaper the background's hue comes from - the side
/// another screen sits on, so the two meet in the same colour. None: the
/// hue of the darkest large region, wherever it is.
#[derive(Clone, Copy, PartialEq)]
pub enum Side {
    None,
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    pub fn parse(s: &str) -> Option<Side> {
        Some(match s {
            "none" => Side::None,
            "left" => Side::Left,
            "right" => Side::Right,
            "top" => Side::Top,
            "bottom" => Side::Bottom,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Side::None => "none",
            Side::Left => "left",
            Side::Right => "right",
            Side::Top => "top",
            Side::Bottom => "bottom",
        }
    }

    fn contains(self, x: usize, y: usize, g: &Grid) -> bool {
        let (fx, fy) = ((x as f64 + 0.5) / g.width as f64, (y as f64 + 0.5) / g.height as f64);
        match self {
            Side::None => false,
            Side::Left => fx < EDGE,
            Side::Right => fx > 1.0 - EDGE,
            Side::Top => fy < EDGE,
            Side::Bottom => fy > 1.0 - EDGE,
        }
    }
}

pub struct Region {
    pub lab: Lab,
    /// Share of the whole image.
    pub area: f64,
    /// Share of the --side edge strip (0 with Side::None).
    pub edge: f64,
}

/// SplitMix64: deterministic seeding for k-means++, nothing more.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn d2(a: &Lab, b: &Lab) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn nearest(p: &Lab, centers: &[Lab]) -> usize {
    let mut best = (f64::MAX, 0);
    for (j, c) in centers.iter().enumerate() {
        let d = d2(p, c);
        if d < best.0 {
            best = (d, j);
        }
    }
    best.1
}

/// k-means++ then Lloyd; regions sorted by area, largest first.
pub fn regions(grid: &Grid, side: Side) -> Vec<Region> {
    let px = &grid.px[..];
    let mut rng = Rng(0);
    let mut centers = vec![px[(rng.next() * px.len() as f64) as usize]];
    let mut dmin: Vec<f64> = px.iter().map(|p| d2(p, &centers[0])).collect();
    while centers.len() < K.min(px.len()) {
        let total: f64 = dmin.iter().sum();
        if total <= 0.0 {
            break;
        }
        let mut t = rng.next() * total;
        let i = dmin.iter().position(|&d| {
            t -= d;
            t <= 0.0
        });
        let c = px[i.unwrap_or(px.len() - 1)];
        for (d, p) in dmin.iter_mut().zip(px) {
            *d = d.min(d2(p, &c));
        }
        centers.push(c);
    }

    let k = centers.len();
    let mut label = vec![0usize; px.len()];
    for _ in 0..30 {
        let mut sum = vec![[0.0f64; 4]; k];
        for (p, l) in px.iter().zip(label.iter_mut()) {
            *l = nearest(p, &centers);
            let s = &mut sum[*l];
            s[0] += p[0];
            s[1] += p[1];
            s[2] += p[2];
            s[3] += 1.0;
        }
        let mut moved = false;
        for (c, s) in centers.iter_mut().zip(&sum) {
            if s[3] > 0.0 {
                let n = [s[0] / s[3], s[1] / s[3], s[2] / s[3]];
                moved |= (0..3).any(|i| (n[i] - c[i]).abs() > 1e-5);
                *c = n;
            }
        }
        if !moved {
            break;
        }
    }
    let mut count = vec![0usize; k];
    let mut edge = vec![0usize; k];
    let mut edge_total = 0;
    for (i, p) in px.iter().enumerate() {
        let j = nearest(p, &centers);
        count[j] += 1;
        if side.contains(i % grid.width, i / grid.width, grid) {
            edge[j] += 1;
            edge_total += 1;
        }
    }
    let mut out: Vec<Region> = centers
        .into_iter()
        .zip(count.into_iter().zip(edge))
        .map(|(lab, (n, e))| Region {
            lab,
            area: n as f64 / px.len() as f64,
            edge: e as f64 / edge_total.max(1) as f64,
        })
        .collect();
    out.sort_by(|a, b| b.area.total_cmp(&a.area));
    out
}

/// Chroma untouched below `k`, softly limited to `k + room` above it.
fn knee(c: f64, k: f64, room: f64) -> f64 {
    if c <= k { c } else { k + room * ((c - k) / room).tanh() }
}

/// Push the closest pair apart until every pair is MIN_DE apart: the darker
/// one down and the lighter one up. Mostly by lightness: hues only get a
/// small nudge, so a colour can't drift into a hue the image doesn't have.
fn separate(labs: &mut [Lab], grey: bool) {
    for _ in 0..60 {
        let mut best = (f64::MAX, 0, 0);
        for a in 0..labs.len() {
            for b in a + 1..labs.len() {
                let d = dist(labs[a], labs[b]);
                if d < best.0 {
                    best = (d, a, b);
                }
            }
        }
        if best.0 >= MIN_DE {
            break;
        }
        let (i, j) = (best.1, best.2);
        let (lo, hi) = if labs[i][0] <= labs[j][0] { (i, j) } else { (j, i) };
        for (t, s) in [(lo, -1.0), (hi, 1.0)] {
            let [l, c, h] = lch(labs[t]);
            labs[t] = if grey {
                from_lch((l + s * 0.02).clamp(0.48, 0.95), c, h)
            } else {
                from_lch((l + s * 0.02).clamp(0.48, 0.92), c.max(0.03), h + s * 2.0 * DEG)
            };
        }
    }
}

pub struct Palette {
    /// The colour covering most of the --side edge, as it is in the image
    /// (None without --side): for whatever sits beside that edge, e.g. the
    /// field of another screen's wallpaper.
    pub edge: Option<Lab>,
    pub background: Lab,
    pub foreground: Lab,
    pub cursor: Lab,
    pub colors: [Lab; 16],
}

pub fn build(regions: &[Region]) -> Palette {
    let lc: Vec<[f64; 3]> = regions.iter().map(|r| lch(r.lab)).collect();
    let area: Vec<f64> = regions.iter().map(|r| r.area).collect();
    let near = |h: f64, hue: f64| (-hue_gap(hue, h).powi(2) / (2.0 * (25.0 * DEG).powi(2))).exp();

    // How much of the image backs hue h: area x chroma, smoothed +-25deg.
    // Near-black regions don't count: their OKLab hue is a few levels of
    // RGB noise (#030006 reads as 308deg), and as the biggest region they
    // outvoted every real colour, turning blue wallpapers' ramps purple.
    let visible = |l: f64| ((l - 0.12) / 0.13).clamp(0.0, 1.0);
    let support = |h: f64| -> f64 {
        lc.iter()
            .zip(&area)
            .map(|(c, a)| a * visible(c[0]) * (c[1] - 0.01).max(0.0) * near(h, c[2]))
            .sum()
    };
    // Support from only the regions whose own hue lies on one side of
    // `from`, more than `gap` away (sign +1 / -1, or 0 for either side).
    // Measuring "other hues" with plain support() counts the smoothing's
    // tail: one blue hue scores 28% of its peak 40deg away, which made
    // every single-hue image look two-hued with a phantom purple family.
    let support_away = |h: f64, from: f64, gap: f64, sign: f64| -> f64 {
        lc.iter()
            .zip(&area)
            .filter(|(c, _)| {
                let d = (c[2] - from + PI).rem_euclid(2.0 * PI) - PI;
                d.abs() > gap && (sign == 0.0 || d * sign > 0.0)
            })
            .map(|(c, a)| a * visible(c[0]) * (c[1] - 0.01).max(0.0) * near(h, c[2]))
            .sum()
    };
    let hues: Vec<f64> = (0..180).map(|i| (2 * i) as f64 * DEG).collect();
    let sup: Vec<f64> = hues.iter().map(|&h| support(h)).collect();
    let (mut h1, mut smax) = (0.0, f64::MIN);
    for (&h, &s) in hues.iter().zip(&sup) {
        if s > smax {
            (h1, smax) = (h, s);
        }
    }

    // Second family: best-supported hue >= 40deg away, counting only
    // regions >= 30deg off the main hue, if it carries weight; otherwise
    // the image is monochrome and gets a near neighbour.
    let (mut v2, mut h2) = (f64::MIN, h1);
    for &h in hues.iter().filter(|&&h| hue_gap(h, h1) >= 40.0 * DEG) {
        let s = support_away(h, h1, 30.0 * DEG, 0.0);
        if (s, h) > (v2, h2) {
            (v2, h2) = (s, h);
        }
    }
    let grey = lc.iter().zip(&area).map(|(c, a)| a * c[1]).sum::<f64>() < ACHROMATIC;
    let mono = v2 < 0.25 * smax;
    if mono {
        let side = if support(h1 + 10.0 * DEG) >= support(h1 - 10.0 * DEG) { 10.0 } else { -10.0 };
        h2 = h1 + side * DEG;
    }
    let family_chroma = |h: f64| {
        let (mut ws, mut cs) = (0.0, 0.0);
        for (c, a) in lc.iter().zip(&area) {
            let w = a * visible(c[0]) * near(h, c[2]);
            ws += w;
            cs += w * c[1];
        }
        cs / ws.max(1e-9)
    };
    let (c1, c2) = (family_chroma(h1), family_chroma(h2));

    // Accents: real regions, by sqrt(area) x chroma, not too dark.
    let score: Vec<f64> = lc
        .iter()
        .zip(&area)
        .map(|(c, &a)| {
            if a < MIN_AREA {
                return 0.0;
            }
            a.sqrt() * c[1] * ((c[0] - 0.2) / 0.2).clamp(0.0, 1.0)
        })
        .collect();
    let mut order: Vec<usize> = (0..lc.len()).collect();
    order.sort_by(|&a, &b| score[b].total_cmp(&score[a]));
    let a1 = order[0];
    let (ha1, ca1) = (lc[a1][2], lc[a1][1]);
    // No second accent region: the second hue family if it's far enough
    // off, else 35deg toward whichever side the image backs, else accent 1's
    // own hue, lighter and softer - never a hue the image doesn't have (a
    // fixed +40deg turned every blue wallpaper's second accent purple).
    let (ha2, ca2, dl2) = match order[1..]
        .iter()
        .find(|&&i| score[i] > 0.0 && hue_gap(lc[i][2], ha1) > 35.0 * DEG)
    {
        Some(&i) => (lc[i][2], lc[i][1], 0.0),
        None if !mono && hue_gap(h2, ha1) > 35.0 * DEG => (h2, 0.8 * ca1, 0.0),
        None => {
            let (up, down) = (
                support_away(ha1 + 35.0 * DEG, ha1, 25.0 * DEG, 1.0),
                support_away(ha1 - 35.0 * DEG, ha1, 25.0 * DEG, -1.0),
            );
            if up.max(down) >= 0.25 * smax {
                let side = if up >= down { 35.0 } else { -35.0 };
                (ha1 + side * DEG, 0.8 * ca1, 0.0)
            } else {
                (ha1, 0.7 * ca1, 0.11)
            }
        }
    };

    let accent_c = |c: f64| knee(c * 1.25, 0.10, 0.05).clamp(0.085, 0.15);
    let ramp_c = |c: f64, n: usize| (c * 1.1).clamp(0.04, 0.10) * (0.8 + 0.1 * n as f64);
    let ramp_h = if mono { [h1, h2, h1, h2] } else { [h1, h2, h1 - 12.0 * DEG, h2 + 12.0 * DEG] };
    let ramp_cs = [c1, c2, c1, c2];
    let mut normal = [[0.0; 3]; 6];
    for n in 0..4 {
        normal[n] = from_lch(RAMP_L[n], ramp_c(ramp_cs[n], n), ramp_h[n]);
    }
    normal[4] = from_lch(ACCENT_L, accent_c(ca1), ha1);
    normal[5] = from_lch(ACCENT_L - 0.03 + dl2, accent_c(ca2), ha2);
    if grey {
        for x in &mut normal[..4] {
            *x = from_lch(x[0], 0.007, h1);
        }
        normal[4] = from_lch(ACCENT_L + 0.06, 0.008, h1);
        normal[5] = from_lch(ACCENT_L + 0.12, 0.006, h1);
    }
    separate(&mut normal, grey);
    let bright: Vec<Lab> = normal
        .iter()
        .enumerate()
        .map(|(n, &x)| {
            let [l, c, h] = lch(x);
            from_lch((l + BRIGHT_DL).min(0.9), c * if n >= 4 { 1.05 } else { 0.9 }, h)
        })
        .collect();

    // Background: lightness and chroma from the darkest region covering
    // >= 5% of the image. Hue from the region covering most of the --side
    // edge if one was asked for; a greyer edge also caps the chroma, so a
    // grey edge doesn't come out tinted.
    let d = (0..regions.len())
        .filter(|&i| area[i] >= 0.05)
        .min_by(|&a, &b| lc[a][0].total_cmp(&lc[b][0]))
        .unwrap_or(0);
    let [bl, mut bc, mut bh] = lc[d];
    let edge = (0..regions.len())
        .filter(|&i| regions[i].edge > 0.0)
        .max_by(|&a, &b| regions[a].edge.total_cmp(&regions[b].edge));
    if let Some(e) = edge {
        bh = lc[e][2];
        bc = bc.min(lc[e][1]);
    }
    let bg = from_lch((bl * BG_L_SCALE).clamp(BG_L.0, BG_L.1), bc.min(0.03), bh);

    let mut colors = [[0.0; 3]; 16];
    colors[0] = bg;
    colors[7] = from_lch(0.80, 0.03, h1);
    colors[8] = from_lch(0.50, 0.035, h1);
    colors[15] = from_lch(0.94, 0.02, h1);
    for i in 0..6 {
        colors[i + 1] = normal[i];
        colors[i + 9] = bright[i];
    }
    Palette {
        edge: edge.map(|e| regions[e].lab),
        background: bg,
        foreground: from_lch(0.90, 0.03, h1),
        cursor: bright[4],
        colors,
    }
}
