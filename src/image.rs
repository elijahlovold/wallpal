//! PNG -> small OKLab grid, box-filtered while decoding.
//!
//! Each output cell is the plain average of the source pixels that fall in
//! it (like PIL's BOX resize), so stars and small lights blend into their
//! surroundings before any colour is picked.

use std::fs::File;
use std::io::BufReader;

use png::{ColorType, Transformations};

use crate::color::{lin_table, linear_to_oklab, Lab};

pub const WIDTH: usize = 128;

/// The downsampled image, row-major.
pub struct Grid {
    pub px: Vec<Lab>,
    pub width: usize,
    pub height: usize,
}

pub fn load(path: &str) -> Result<Grid, String> {
    let file = File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let mut dec = png::Decoder::new(BufReader::with_capacity(1 << 16, file));
    // 16-bit -> 8, palette/low-bit -> 8-bit channels
    dec.set_transformations(Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| format!("{path}: {e}"))?;
    let (w, h) = (reader.info().width as usize, reader.info().height as usize);
    let channels = match reader.output_color_type().0 {
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Indexed => return Err(format!("{path}: unexpanded palette")),
    };
    let ow = WIDTH.min(w);
    let oh = ((h as f64 * ow as f64 / w as f64).round() as usize).max(1);
    // Source column range of each output column: cells are contiguous runs,
    // so the hot loop is a plain sum over a slice, no per-pixel lookup.
    let runs: Vec<(usize, usize)> = (0..ow).map(|c| ((c * w).div_ceil(ow), ((c + 1) * w).div_ceil(ow))).collect();
    // r, g, b, count per cell
    let mut acc = vec![[0u32; 4]; ow * oh];

    let mut add_row = |y: usize, data: &[u8]| {
        let row = &mut acc[(y * oh / h) * ow..][..ow];
        match channels {
            3 => sum_row::<3>(row, &runs, data),
            4 => sum_row::<4>(row, &runs, data),
            2 => sum_row::<2>(row, &runs, data),
            _ => sum_row::<1>(row, &runs, data),
        }
    };

    if reader.info().interlaced {
        // rare for wallpapers; decode the whole frame instead of streaming
        let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
        let frame = reader.next_frame(&mut buf).map_err(|e| format!("{path}: {e}"))?;
        for (y, line) in buf.chunks_exact(frame.line_size).take(h).enumerate() {
            add_row(y, line);
        }
    } else {
        let mut y = 0;
        while let Some(row) = reader.next_row().map_err(|e| format!("{path}: {e}"))? {
            add_row(y, row.data());
            y += 1;
        }
    }

    // round to bytes like a resized 8-bit image would, then to OKLab;
    // grey images summed only channel 0
    if channels < 3 {
        for c in &mut acc {
            (c[1], c[2]) = (c[0], c[0]);
        }
    }
    let lin = lin_table();
    let px = acc
        .iter()
        .map(|&[r, g, b, n]| {
            let n = n.max(1) as f64;
            let ch = |s: u32| lin[(s as f64 / n).round() as usize];
            linear_to_oklab([ch(r), ch(g), ch(b)])
        })
        .collect();
    Ok(Grid { px, width: ow, height: oh })
}

/// Add one decoded row into its output cells. CH is bytes per pixel; for
/// grey (1-2) only channel 0 is summed and copied to g/b afterwards.
fn sum_row<const CH: usize>(row: &mut [[u32; 4]], runs: &[(usize, usize)], data: &[u8]) {
    for (cell, &(x0, x1)) in row.iter_mut().zip(runs) {
        let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
        for px in data[x0 * CH..x1 * CH].chunks_exact(CH) {
            r += px[0] as u32;
            if CH >= 3 {
                g += px[1] as u32;
                b += px[2] as u32;
            }
        }
        cell[0] += r;
        cell[1] += g;
        cell[2] += b;
        cell[3] += (x1 - x0) as u32;
    }
}
