//! Tiles upscaled with generated normal maps and surface data (the port of `enhance.ts`): each
//! tile is grown four times (Scale2x twice by default), painted, and given a height (a dome for
//! sprites, the brightness against its part's mean for floors and walls), a normal map with the
//! height in blue, and a surface map (red gloss, green glow, blue sparkle, alpha see-through).

use crate::labels::{label_of, tile_hash, Joints, TileLabel, UNLABELLED};
use crate::lights::{PaintSettings, Upscale};
use crate::scalefx::{scalefx, SCALEFX_SCALE};

pub const ENHANCE_SCALE: u32 = 4;
const DOME_RADIUS: f32 = 10.0;
const OUTLINE_TRIM: f32 = 2.0;
const OUTLINE_LUMINANCE: f32 = 40.0;
const RELIEF: f32 = 1.6;
const TERRAIN_RELIEF: f32 = 6.0;
const RELIEF_AMPLITUDE: [f32; 4] = [0.08, 0.35, 0.7, 1.0];
const GLOSS: [f32; 4] = [0.0, 0.3, 0.6, 1.0];
const WET_GLOSS: f32 = 0.65;

fn joint_span(j: Joints) -> u32 {
    match j {
        Joints::None => 3,
        Joints::Bricks => 4,
        Joints::Blocks => 6,
        Joints::Cobbles => 4,
        Joints::Planks => 3,
        Joints::Tiles => 5,
        Joints::Cracks => 2,
    }
}

fn solid_parts(j: Joints) -> bool {
    matches!(j, Joints::Bricks | Joints::Blocks | Joints::Cobbles | Joints::Tiles)
}

/// Pixels as little-endian `u32` (r in the low byte), as the JS reads an RGBA buffer.
fn to_words(rgba: &[u8]) -> Vec<u32> {
    rgba.chunks_exact(4).map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]])).collect()
}

fn to_bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Scale2x (EPX): doubles pixel art, rounding diagonal steps without blurring.
fn scale2x(src: &[u32], w: usize, h: usize) -> Vec<u32> {
    let mut out = vec![0u32; w * h * 4];
    let at = |x: isize, y: isize| src[(y.clamp(0, h as isize - 1) as usize) * w + x.clamp(0, w as isize - 1) as usize];
    let ow = w * 2;
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x as isize, y as isize);
            let p = at(xi, yi);
            let a = at(xi, yi - 1);
            let b = at(xi + 1, yi);
            let c = at(xi - 1, yi);
            let d = at(xi, yi + 1);
            let o = y * 2 * ow + x * 2;
            out[o] = if c == a && c != d && a != b { a } else { p };
            out[o + 1] = if a == b && a != c && b != d { b } else { p };
            out[o + ow] = if d == c && d != b && c != a { c } else { p };
            out[o + ow + 1] = if b == d && b != a && d != c { d } else { p };
        }
    }
    out
}

fn nearest_scale(src: &[u32], w: usize, h: usize, factor: usize) -> Vec<u32> {
    let ow = w * factor;
    let mut out = vec![0u32; ow * h * factor];
    for y in 0..h * factor {
        for x in 0..ow {
            out[y * ow + x] = src[(y / factor) * w + x / factor];
        }
    }
    out
}

fn channel(p: u32, shift: u32) -> i32 {
    ((p >> shift) & 255) as i32
}

fn pixel_distance(a: u32, b: u32) -> f32 {
    if a == b {
        return 0.0;
    }
    let dr = (channel(a, 0) - channel(b, 0)) as f32;
    let dg = (channel(a, 8) - channel(b, 8)) as f32;
    let db = (channel(a, 16) - channel(b, 16)) as f32;
    let da = (channel(a, 24) - channel(b, 24)) as f32;
    let y = (0.299 * dr + 0.587 * dg + 0.114 * db).abs();
    let u = (-0.169 * dr - 0.331 * dg + 0.5 * db).abs();
    let v = (0.5 * dr - 0.419 * dg - 0.081 * db).abs();
    48.0 * y + 7.0 * u + 6.0 * v + 64.0 * da.abs()
}

fn blend_pixels(a: u32, b: u32) -> u32 {
    let aa = channel(a, 24);
    let ba = channel(b, 24);
    let alpha = aa + ba;
    if alpha == 0 {
        return 0;
    }
    let mix = |s: u32| ((channel(a, s) * aa + channel(b, s) * ba) as f32 / alpha as f32).round() as u32;
    mix(0) | (mix(8) << 8) | (mix(16) << 16) | (((alpha as f32 / 2.0).round() as u32) << 24)
}

const XBR_CORNERS: [(isize, isize); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];

/// xBR at 2x: each corner of a pixel takes the colour across an edge when the edge runs along it.
fn xbr2x(src: &[u32], w: usize, h: usize) -> Vec<u32> {
    let mut out = vec![0u32; w * h * 4];
    let ow = w * 2;
    for y in 0..h {
        for x in 0..w {
            let e = src[y * w + x];
            for (sx, sy) in XBR_CORNERS {
                let at = |dx: isize, dy: isize| {
                    let px = (x as isize + if sx == sy { dx * sx } else { dy * sx }).clamp(0, w as isize - 1) as usize;
                    let py = (y as isize + if sx == sy { dy * sy } else { dx * sy }).clamp(0, h as isize - 1) as usize;
                    src[py * w + px]
                };
                let f = at(1, 0);
                let hh = at(0, 1);
                let mut corner = e;
                if e != f && e != hh {
                    let i = at(1, 1);
                    let across = pixel_distance(e, at(1, -1))
                        + pixel_distance(e, at(-1, 1))
                        + pixel_distance(i, at(2, 0))
                        + pixel_distance(i, at(0, 2))
                        + 4.0 * pixel_distance(hh, f);
                    let along = pixel_distance(hh, at(-1, 0))
                        + pixel_distance(hh, at(1, 2))
                        + pixel_distance(f, at(2, 1))
                        + pixel_distance(f, at(0, -1))
                        + 4.0 * pixel_distance(e, i);
                    if across < along {
                        corner = blend_pixels(e, if pixel_distance(e, f) <= pixel_distance(e, hh) { f } else { hh });
                    }
                }
                out[(y * 2 + (sy > 0) as usize) * ow + x * 2 + (sx > 0) as usize] = corner;
            }
        }
    }
    out
}

/// How many times larger a way of upscaling makes the tiles.
pub fn upscale_factor(how: Upscale) -> u32 {
    if how == Upscale::ScaleFx {
        SCALEFX_SCALE
    } else {
        ENHANCE_SCALE
    }
}

/// A tile's pixels grown `upscale_factor` times in the chosen way.
pub fn upscale(rgba: &[u8], w: u32, h: u32, how: Upscale) -> Vec<u8> {
    let data = to_words(rgba);
    let (w, h) = (w as usize, h as usize);
    let out = match how {
        Upscale::Pixel => nearest_scale(&data, w, h, ENHANCE_SCALE as usize),
        Upscale::ScaleFx => scalefx(&data, w, h),
        Upscale::Xbr => xbr2x(&xbr2x(&data, w, h), w * 2, h * 2),
        Upscale::Scale2x => scale2x(&scale2x(&data, w, h), w * 2, h * 2),
    };
    to_bytes(&out)
}

/// Distance in pixels to the nearest transparent pixel, capped at `cap` (two-pass chamfer).
fn edge_distance(px: &[u8], w: usize, h: usize, cap: f32) -> Vec<f32> {
    let mut d: Vec<f32> = (0..w * h).map(|i| if px[i * 4 + 3] < 128 { 0.0 } else { cap }).collect();
    let diag = std::f32::consts::SQRT_2;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if d[i] == 0.0 {
                continue;
            }
            if x > 0 {
                d[i] = d[i].min(d[i - 1] + 1.0)
            }
            if y > 0 {
                d[i] = d[i].min(d[i - w] + 1.0)
            }
            if x > 0 && y > 0 {
                d[i] = d[i].min(d[i - w - 1] + diag)
            }
            if x < w - 1 && y > 0 {
                d[i] = d[i].min(d[i - w + 1] + diag)
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if d[i] == 0.0 {
                continue;
            }
            if x < w - 1 {
                d[i] = d[i].min(d[i + 1] + 1.0)
            }
            if y < h - 1 {
                d[i] = d[i].min(d[i + w] + 1.0)
            }
            if x < w - 1 && y < h - 1 {
                d[i] = d[i].min(d[i + w + 1] + diag)
            }
            if x > 0 && y < h - 1 {
                d[i] = d[i].min(d[i + w - 1] + diag)
            }
        }
    }
    d
}

/// DCSS outlines every sprite in one black pixel, four after upscaling; trim the outer half.
fn trim_outline(px: &mut [u8], w: usize, h: usize) {
    let distance = edge_distance(px, w, h, OUTLINE_TRIM + 1.0);
    for i in 0..w * h {
        let o = i * 4;
        let l = 0.299 * px[o] as f32 + 0.587 * px[o + 1] as f32 + 0.114 * px[o + 2] as f32;
        if px[o + 3] >= 128 && distance[i] <= OUTLINE_TRIM && l < OUTLINE_LUMINANCE {
            px[o + 3] = 0;
        }
    }
}

/// Kuwahara filter on the opaque texels, with summed-area tables.
fn kuwahara(px: &mut [u8], w: usize, h: usize, radius: usize) {
    let stride = w + 1;
    let mut sums = vec![vec![0f64; stride * (h + 1)]; 6];
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 4;
            let opaque = if px[o + 3] >= 128 { 1.0 } else { 0.0 };
            let l = 0.299 * px[o] as f64 + 0.587 * px[o + 1] as f64 + 0.114 * px[o + 2] as f64;
            let values = [px[o] as f64 * opaque, px[o + 1] as f64 * opaque, px[o + 2] as f64 * opaque, l * opaque, l * l * opaque, opaque];
            let i = (y + 1) * stride + x + 1;
            for (k, sum) in sums.iter_mut().enumerate() {
                sum[i] = values[k] + sum[i - 1] + sum[i - stride] - sum[i - stride - 1];
            }
        }
    }
    let area = |s: &[f64], x0: usize, y0: usize, x1: usize, y1: usize| s[y1 * stride + x1] - s[y0 * stride + x1] - s[y1 * stride + x0] + s[y0 * stride + x0];
    let mut out = px.to_vec();
    let r = radius as isize;
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 4;
            if px[o + 3] < 128 {
                continue;
            }
            let mut best = f64::INFINITY;
            let (xi, yi) = (x as isize, y as isize);
            for (qx, qy) in [(xi - r, yi - r), (xi, yi - r), (xi - r, yi), (xi, yi)] {
                let x0 = qx.max(0) as usize;
                let y0 = qy.max(0) as usize;
                let x1 = ((qx + r + 1) as usize).min(w);
                let y1 = ((qy + r + 1) as usize).min(h);
                let n = area(&sums[5], x0, y0, x1, y1);
                if n <= 0.0 {
                    continue;
                }
                let mean = area(&sums[3], x0, y0, x1, y1) / n;
                let variance = area(&sums[4], x0, y0, x1, y1) / n - mean * mean;
                if variance >= best {
                    continue;
                }
                best = variance;
                out[o] = clamp8((area(&sums[0], x0, y0, x1, y1) / n) as f32);
                out[o + 1] = clamp8((area(&sums[1], x0, y0, x1, y1) / n) as f32);
                out[o + 2] = clamp8((area(&sums[2], x0, y0, x1, y1) / n) as f32);
            }
        }
    }
    px.copy_from_slice(&out);
}

fn lift(px: &mut [u8], amount: f32) {
    let curve: Vec<u8> = (0..256).map(|v| clamp8(255.0 * (v as f32 / 255.0).powf(1.0 / amount))).collect();
    for p in px.chunks_exact_mut(4) {
        for c in &mut p[..3] {
            *c = curve[*c as usize];
        }
    }
}

fn posterize(px: &mut [u8], levels: u32) {
    let step = 255.0 / (levels.max(1) - 1).max(1) as f32;
    for p in px.chunks_exact_mut(4) {
        for c in &mut p[..3] {
            *c = clamp8((*c as f32 / step).round() * step);
        }
    }
}

/// A value stored as a `Uint8ClampedArray` stores it: clamped and rounded.
fn clamp8(v: f32) -> u8 {
    v.clamp(0.0, 255.0).round_ties_even() as u8
}

fn luminance(px: &[u8], i: usize) -> f32 {
    (0.299 * px[i * 4] as f32 + 0.587 * px[i * 4 + 1] as f32 + 0.114 * px[i * 4 + 2] as f32) / 255.0
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The height of a sprite: a rounded dome from the silhouette plus its brightness.
fn sprite_height(px: &[u8], w: usize, h: usize) -> Vec<f32> {
    let distance = edge_distance(px, w, h, DOME_RADIUS);
    (0..w * h)
        .map(|i| {
            let t = distance[i] / DOME_RADIUS;
            let dome = (1.0 - (1.0 - t) * (1.0 - t)).max(0.0).sqrt();
            dome * 0.4 + luminance(px, i) * 0.35
        })
        .collect()
}

/// A box blur of `radius` texels across and down, wrapping at the edges.
fn box_blur(values: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    if radius < 1 {
        return values.to_vec();
    }
    let size = (radius * 2 + 1) as f32;
    let wrap = |v: isize, n: usize| v.rem_euclid(n as isize) as usize;
    let r = radius as isize;
    let mut across = vec![0f32; w * h];
    for y in 0..h {
        let mut sum = 0.0;
        for k in -r..=r {
            sum += values[y * w + wrap(k, w)];
        }
        for x in 0..w {
            across[y * w + x] = sum / size;
            sum += values[y * w + wrap(x as isize + r + 1, w)] - values[y * w + wrap(x as isize - r, w)];
        }
    }
    let mut out = vec![0f32; w * h];
    for x in 0..w {
        let mut sum = 0.0;
        for k in -r..=r {
            sum += across[wrap(k, h) * w + x];
        }
        for y in 0..h {
            out[y * w + x] = sum / size;
            sum += across[wrap(y as isize + r + 1, h) * w + x] - across[wrap(y as isize - r, h) * w + x];
        }
    }
    out
}

/// The height of a floor or a wall tile, 0 to 1 around 0.5, by its label.
fn terrain_height(px: &[u8], w: usize, h: usize, scale: u32, label: &TileLabel) -> Vec<f32> {
    let bright: Vec<f32> = (0..w * h).map(|i| if label.bright_is_high { luminance(px, i) } else { 1.0 - luminance(px, i) }).collect();
    let half = (scale >> 1).max(1) as usize;
    let fine = box_blur(&box_blur(&bright, w, h, half), w, h, half);
    let span = joint_span(label.joints) * scale;
    let broad = box_blur(&box_blur(&bright, w, h, (span >> 1) as usize), w, h, (span >> 1) as usize);
    let band: Vec<f32> = fine.iter().zip(&broad).map(|(f, b)| f - b).collect();
    let mut spread = (band.iter().map(|v| v * v).sum::<f32>() / band.len() as f32).sqrt() * 2.5;
    if spread == 0.0 || !spread.is_finite() {
        spread = 1.0;
    }
    let solid = solid_parts(label.joints);
    let amplitude = RELIEF_AMPLITUDE.get(label.relief as usize).copied().unwrap_or(RELIEF_AMPLITUDE[1]);
    let t2 = 2f32.tanh();
    band.iter()
        .map(|v| {
            let t = (v / spread).clamp(-1.0, 1.0);
            let shaped = if solid { (t * 2.0).tanh() / t2 } else { t };
            0.5 + shaped * 0.5 * amplitude
        })
        .collect()
}

/// A tangent-space normal map from a height, bent by `relief`; blue is the parallax height.
fn normal_map(height: &[f32], depth: &[f32], w: usize, h: usize, relief: f32) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let at = |x: isize, y: isize| height[(y.clamp(0, h as isize - 1) as usize) * w + x.clamp(0, w as isize - 1) as usize];
    for y in 0..h {
        for x in 0..w {
            let (xi, yi) = (x as isize, y as isize);
            let dx = (at(xi + 1, yi) - at(xi - 1, yi)) * relief;
            let dy = (at(xi, yi + 1) - at(xi, yi - 1)) * relief;
            let len = (dx * dx + dy * dy + 1.0).sqrt();
            let o = (y * w + x) * 4;
            out[o] = clamp8(((-dx / len) * 0.5 + 0.5) * 255.0);
            out[o + 1] = clamp8(((dy / len) * 0.5 + 0.5) * 255.0);
            out[o + 2] = clamp8(depth[y * w + x] * 255.0);
            out[o + 3] = 255;
        }
    }
    out
}

/// What each texel's surface does: red gloss, green glow, blue sparkle, alpha see-through.
fn surface_map(px: &[u8], w: usize, h: usize, label: &TileLabel) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let gloss = GLOSS.get(label.gloss as usize).copied().unwrap_or(0.0).max(if label.wet { WET_GLOSS } else { 0.0 });
    for i in 0..w * h {
        let r = px[i * 4] as f32 / 255.0;
        let g = px[i * 4 + 1] as f32 / 255.0;
        let b = px[i * 4 + 2] as f32 / 255.0;
        let value = r.max(g).max(b);
        let saturation = if value > 0.0 { (value - r.min(g).min(b)) / value } else { 0.0 };
        let light = luminance(px, i);
        let glow = if label.glow >= 2 {
            smoothstep(0.15, 0.7, value)
        } else if label.glow == 1 {
            smoothstep(0.55, 0.85, value) * smoothstep(0.3, 0.65, saturation)
        } else {
            0.0
        };
        out[i * 4] = clamp8(gloss * (0.75 + 0.5 * (light - 0.5)) * 255.0);
        out[i * 4 + 1] = clamp8(glow * 255.0);
        out[i * 4 + 2] = if label.sparkle { clamp8(smoothstep(0.55, 0.9, light) * 255.0) } else { 0 };
        out[i * 4 + 3] = if label.translucent { 255 } else { 0 };
    }
    out
}

/// The height spread over 0 to 1 between the lowest and the highest opaque texel.
fn tile_depth(px: &[u8], height: &[f32]) -> Vec<f32> {
    let opaque = |i: usize| px[i * 4 + 3] >= 128;
    let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
    for (i, &h) in height.iter().enumerate() {
        if opaque(i) {
            low = low.min(h);
            high = high.max(h);
        }
    }
    let range = high - low;
    height.iter().enumerate().map(|(i, &h)| if opaque(i) && range > 0.0 { (h - low) / range } else { 0.0 }).collect()
}

/// What `enhance_tile` works from.
#[derive(Clone, Debug)]
pub struct TileJob {
    pub pixels: Vec<u8>,
    pub w: u32,
    pub h: u32,
    /// A sprite cut from its cell.
    pub framed: bool,
    pub paint: PaintSettings,
    pub label: Option<TileLabel>,
}

/// A tile's upscaled colour, its normal map and its surface data, `w` by `h`.
#[derive(Clone, Debug)]
pub struct EnhancedTile {
    pub w: u32,
    pub h: u32,
    pub colour: Vec<u8>,
    pub normal: Vec<u8>,
    pub surface: Vec<u8>,
}

/// Upscale a tile, paint it, and make its normal map and surface data.
pub fn enhance_tile(job: &TileJob) -> EnhancedTile {
    let scale = upscale_factor(job.paint.upscale);
    let mut colour = upscale(&job.pixels, job.w, job.h, job.paint.upscale);
    let (w, h) = ((job.w * scale) as usize, (job.h * scale) as usize);
    let sprite = job.framed || colour.chunks_exact(4).any(|p| p[3] < 128);
    if sprite {
        trim_outline(&mut colour, w, h);
    }
    if job.paint.lift != 1.0 {
        lift(&mut colour, job.paint.lift);
    }
    if job.paint.radius > 0.0 {
        kuwahara(&mut colour, w, h, job.paint.radius.round() as usize);
    }
    if job.paint.levels > 1.0 {
        posterize(&mut colour, job.paint.levels.round() as u32);
    }
    let label = job.label.unwrap_or(UNLABELLED);
    let surface = surface_map(&colour, w, h, &label);
    let normal = if sprite {
        let height = sprite_height(&colour, w, h);
        normal_map(&height, &tile_depth(&colour, &height), w, h, RELIEF)
    } else {
        let height = terrain_height(&colour, w, h, scale, &label);
        normal_map(&height, &height, w, h, TERRAIN_RELIEF)
    };
    EnhancedTile { w: w as u32, h: h as u32, colour, normal, surface }
}

/// An image grown `upscale_factor` times, for pixel art shown large outside the dungeon.
pub fn upscale_image(rgba: &[u8], w: u32, h: u32, how: Upscale) -> (Vec<u8>, u32, u32) {
    let f = upscale_factor(how);
    (upscale(rgba, w, h, how), w * f, h * f)
}

/// How many source texels across and down one page of upscaled tiles holds.
pub const PAGE_TEXELS: u32 = 1024;
const PAGE_GUTTER: u32 = 2;

/// A tile's place on a page, in source texels and in page texels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSlot {
    pub sx: u32,
    pub sy: u32,
    pub x: u32,
    pub y: u32,
}

/// A page of upscaled tiles, filled one tile at a time in rows from the top.
#[derive(Clone, Debug)]
pub struct EnhancedPage {
    pub size: u32,
    scale: u32,
    texels: u32,
    row: (u32, u32, u32),
}

impl EnhancedPage {
    pub fn new(paint: &PaintSettings) -> Self {
        let scale = upscale_factor(paint.upscale);
        EnhancedPage { size: PAGE_TEXELS * scale, scale, texels: PAGE_TEXELS, row: (0, 0, 0) }
    }

    /// Hold fewer source texels than `PAGE_TEXELS`, for a GPU with a smaller texture limit.
    pub fn limit(&mut self, texels: u32) {
        self.texels = texels.min(PAGE_TEXELS);
    }

    /// A place on the page for a tile `w` by `h` source texels, or none when the page is full.
    pub fn allocate(&mut self, w: u32, h: u32) -> Option<PageSlot> {
        let texels = self.texels;
        if w + PAGE_GUTTER > texels || h + PAGE_GUTTER > texels {
            return None;
        }
        if self.row.0 + w + PAGE_GUTTER > texels {
            self.row = (0, self.row.1 + self.row.2, 0);
        }
        if self.row.1 + h + PAGE_GUTTER > texels {
            return None;
        }
        let (sx, sy) = (self.row.0 + PAGE_GUTTER / 2, self.row.1 + PAGE_GUTTER / 2);
        self.row = (self.row.0 + w + PAGE_GUTTER, self.row.1, self.row.2.max(h + PAGE_GUTTER));
        Some(PageSlot { sx, sy, x: sx * self.scale, y: sy * self.scale })
    }
}

/// The label of a tile's pixels, by its hash.
pub fn label_for(rgba: &[u8], w: u32, h: u32) -> Option<TileLabel> {
    label_of(tile_hash(rgba, w, h))
}
