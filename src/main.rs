//! wallpal - wallpaper PNG -> 16-colour palette as a wallust colorscheme.
//!
//!     wallpal <image.png> [-o out.json] [--side left|right|top|bottom|none] [--no-cache]
//!     wallpal still.png -o ~/.cache/wallpal/current.json && wallust cs ~/.cache/wallpal/current.json
//!
//! Prints the JSON (background, foreground, cursor, color0-15) to stdout,
//! or writes it to -o atomically. Results are cached under
//! $XDG_CACHE_HOME/wallpal keyed by path, size and mtime, so re-showing a
//! wallpaper skips the PNG decode, which is most of the cost. The key also
//! covers this binary's own size and mtime, so any rebuild invalidates it.

mod color;
mod image;
mod palette;

use std::fs;
use std::hash::Hasher;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::UNIX_EPOCH;

const USAGE: &str = "usage: wallpal <image.png> [-o out.json] [--side left|right|top|bottom|none] [--no-cache]";

/// FNV-1a: a stable key for cache file names (std's hasher is seeded per
/// process).
struct Fnv(u64);

impl Hasher for Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100000001b3);
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

/// A file's size and mtime, for cache keys.
fn identity(path: &std::path::Path) -> Option<(u64, u128)> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some((meta.len(), mtime.as_nanos()))
}

fn cache_file(path: &str, side: palette::Side) -> Option<PathBuf> {
    let abs = fs::canonicalize(path).ok()?;
    let (len, mtime) = identity(&abs)?;
    // this build: a new binary means possibly different palettes
    let (exe_len, exe_mtime) = identity(&std::env::current_exe().ok()?)?;
    let mut h = Fnv(0xcbf29ce484222325);
    h.write(&exe_len.to_le_bytes());
    h.write(&exe_mtime.to_le_bytes());
    h.write(side.name().as_bytes());
    h.write(abs.as_os_str().as_encoded_bytes());
    h.write(&len.to_le_bytes());
    h.write(&mtime.to_le_bytes());
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(dir.join("wallpal").join(format!("{:016x}.json", h.finish())))
}

/// Write via a temp file + rename, so watchers never see a partial file.
fn write_atomic(path: &PathBuf, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

fn to_json(p: &palette::Palette) -> String {
    let mut s = format!(
        "{{\n  \"background\": \"{}\",\n  \"foreground\": \"{}\",\n  \"cursor\": \"{}\"",
        color::hex(p.background),
        color::hex(p.foreground),
        color::hex(p.cursor)
    );
    for (i, c) in p.colors.iter().enumerate() {
        s += &format!(",\n  \"color{i}\": \"{}\"", color::hex(*c));
    }
    // not a wallust key; `wallust cs` ignores it
    if let Some(e) = p.edge {
        s += &format!(",\n  \"edge\": \"{}\"", color::hex(e));
    }
    s + "\n}\n"
}

fn run() -> Result<(), String> {
    let mut image = None;
    let mut out = None;
    let mut use_cache = true;
    let mut side = palette::Side::None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" => out = Some(PathBuf::from(args.next().ok_or(USAGE)?)),
            "--no-cache" => use_cache = false,
            "--side" => side = args.next().as_deref().and_then(palette::Side::parse).ok_or(USAGE)?,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ if image.is_none() && !a.starts_with('-') => image = Some(a),
            _ => return Err(USAGE.into()),
        }
    }
    let image = image.ok_or(USAGE)?;

    let cache = if use_cache { cache_file(&image, side) } else { None };
    let json = match cache.as_ref().and_then(|c| fs::read_to_string(c).ok()) {
        Some(hit) => hit,
        None => {
            let grid = image::load(&image)?;
            let json = to_json(&palette::build(&palette::regions(&grid, side)));
            if let Some(c) = &cache {
                // a cache that can't be written just means no cache
                let _ = write_atomic(c, &json);
            }
            json
        }
    };

    match out {
        Some(o) => write_atomic(&o, &json).map_err(|e| format!("{}: {e}", o.display())),
        None => {
            print!("{json}");
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wallpal: {e}");
            ExitCode::FAILURE
        }
    }
}
