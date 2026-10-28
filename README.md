# wallpal

Wallpaper PNG → 16-colour terminal/desktop palette, written as a
[wallust](https://codeberg.org/explosion-mental/wallust) colorscheme so wallust
can render every template from it.

```sh
wallpal wallpaper.png -o ~/.cache/wallpal/current.json --side left
wallust cs ~/.cache/wallpal/current.json
```

It makes the palette and nothing else. Templates, terminal sequences and
the rest are still wallust's job.

## Why not just `wallust run`

Wallust's palettes work well on varied images but go wrong in a few
predictable ways:

- **Monochrome images give a grey ramp.** On a dark blue wallpaper, color1–14
  come out nearly identical (smallest gap between color1–6 under ΔE 0.04 on 87
  of the 92 wallpapers this was tuned on).
- **Vivid images give neon.** A saturated sunset turns the whole palette
  orange.
- **Small details pick the accents.** A few bright pixels (stars, a lamp)
  can become the strongest colour in the palette.

wallpal keeps wallust's layout — a dark-to-light ramp plus a couple of vivid
accents, with color13 as the accent — but builds it so these cases can't
happen: on the same set, color1–6 stay at least ΔE 0.056 apart (median
0.069).

## Usage

```
wallpal <image.png> [-o out.json] [--side left|right|top|bottom|none] [--no-cache]
```

| Option | |
|---|---|
| `-o FILE` | Write the JSON to `FILE` (atomically: temp file + rename) instead of stdout. |
| `--side S` | The edge another screen sits beside (the outer 20% of the image). Adds an `edge` key: the colour covering most of that edge, as it is in the image, for colouring whatever sits beside it. Also gives `background` that colour's hue. Default `none`: no `edge`, `background` hue from the darkest large region. |
| `--no-cache` | Don't read or write the cache. |

Output (`background`, `foreground`, `cursor`, `color0`–`color15`, plus
`edge` with `--side`; all `#RRGGBB`) is the flat format `wallust cs` reads.
It ignores `edge`:

```json
{
  "background": "#070D1A",
  "foreground": "#D5DEF3",
  "cursor": "#BBD6FF",
  "color0": "#070D1A",
  ...
  "color15": "#E5EBF9",
  "edge": "#1E2138"
}
```

### With wallust

`wallust cs <file>` renders all templates from the palette. Note that as of
wallust 3.5, `cs` **does not run `[hooks]`** (only `wallust run` does), even
though it accepts `--no-hooks`. If you rely on hooks, run them yourself after
`cs`. The `hypr-theme` script in dotfiles-hypr does this by reading the
`[hooks]` table out of `wallust.toml`.

## How it works

Everything happens in [OKLab](https://bottosson.github.io/posts/oklab/).

1. **Shrink.** The PNG is decoded row by row and averaged into a grid 128
   cells wide. Stars, lights and other small details blend into their
   surroundings here, before any colour is picked. The full image is
   never held in memory.
2. **Regions.** k-means (k = 12, deterministic k-means++ seeding) splits the
   grid into colour regions, each with its share of the image.
3. **Hue families.** Each hue gets a score: the area × chroma of regions near it
   (±25°). Near-black regions (lightness under ~0.25) don't count: their
   OKLab hue is mostly RGB noise. The best hue is the main family. The
   second family is the best hue ≥ 40° away, counting only regions that are
   themselves ≥ 30° off the main hue, if it scores at least 25% of the main
   one. (Counting all regions would let the main hue's own ±25° smoothing
   tail pass that test, inventing a second family.) Otherwise the image
   counts as monochrome and the second family is the main ±10°.
4. **Grey images stay grey.** If the image's area-weighted chroma is under
   0.018, the palette is neutral.
5. **Accents** come from real regions, scored by √area × chroma; regions
   under 1.2% of the image and very dark ones can't be accents. Accent 1 →
   color5/13; accent 2 (the next one > 35° of hue away) → color6/14. With no
   such region, accent 2 is the second family's hue; failing that, 35°
   toward whichever side the image actually has colour on; failing that,
   accent 1's own hue, lighter and softer. It never gets a hue the image
   doesn't have.
6. **Tone.**
   - **Ramp** (color1–4): lightness 0.50 / 0.56 / 0.62 / 0.67, alternating the
     two families, chroma 0.04–0.10.
   - **Accents:** lightness ≈ 0.70, chroma 0.085–0.15, softly capped (`tanh`
     knee) so neon is impossible.
   - **Separation:** any pair of color1–6 closer than ΔE 0.065 gets pushed
     apart, mostly by lightness. Hues only get a 2° nudge per step, so
     colours can't drift into another hue family.
   - **Brights** (color9–14): the same colours, lightness +0.15.
7. **Specials.**
   - `background`/`color0`: lightness and chroma from the darkest region
     covering ≥ 5% of the image (lightness 0.08–0.15, chroma ≤ 0.03); hue from
     the `--side` edge if given, which can also lower the chroma so a grey
     edge stays grey.
   - `foreground`, `color7`, `color8`, `color15`: fixed lightness
     (0.90 / 0.80 / 0.50 / 0.94), tinted with the main hue.

Colours outside sRGB are brought in by reducing chroma at fixed lightness and
hue.

## Speed

On 92 wallpapers (mostly 4K PNGs), including process startup:

| | per wallpaper |
|---|---|
| first run | 43 ms median, 57 ms p90 |
| repeat (cached) | ~1 ms |
| `wallust run` (fastresize), for comparison | 50–160 ms |

About 30 ms of a 4K run is the PNG decode itself (zlib inflate is
sequential), 5 ms the averaging, 2–3 ms k-means, and under 0.1 ms the
palette.

**Cache.** Results are stored in `$XDG_CACHE_HOME/wallpal/` (default
`~/.cache/wallpal/`), keyed by the image's canonical path, size, mtime and
`--side`, and by the wallpal binary's own size and mtime, so installing a new
build invalidates every entry automatically. Stale entries are never read;
delete the directory to reclaim the space.

## Build

```sh
cargo build --release              # target/release/wallpal
cargo install --path . --root ~/.local   # or install to ~/.local/bin
```

One dependency: the [`png`](https://crates.io/crates/png) crate, chosen over
`zune-png` because it decodes row by row and benched faster here (32 vs
39 ms on a 4K still). There's no serde or clap: the JSON and argument parsing are a
few lines each. The release profile uses LTO, one codegen unit,
`panic = "abort"` and strips the binary (~460 KB).

PNG only; 8- and 16-bit, RGB(A), grey and palette images are supported.
