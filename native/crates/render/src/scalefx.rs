//! A port of ScaleFX (passes 0 to 4) by Sp00kyFox, from libretro/slang-shaders, run once per tile on
//! the CPU (the port of `scalefx.ts`).
//!
//! Copyright (c) 2016 Sp00kyFox - ScaleFX@web.de
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software
//! and associated documentation files (the "Software"), to deal in the Software without
//! restriction, including without limitation the rights to use, copy, modify, merge, publish,
//! distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
//! Software is furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
//! BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

pub const SCALEFX_SCALE: u32 = 3;
const THRESHOLD: f32 = 0.5;

type V4 = [f32; 4];
type B4 = [bool; 4];

fn gt(a: f32, b: f32) -> f32 {
    (a > b) as u8 as f32
}
fn lt(a: f32, b: f32) -> f32 {
    (a < b) as u8 as f32
}
fn le(a: f32, b: f32) -> f32 {
    (a <= b) as u8 as f32
}
fn not(a: f32) -> f32 {
    1.0 - a
}
fn yzwx(v: V4) -> V4 {
    [v[1], v[2], v[3], v[0]]
}
fn wxyz(v: V4) -> V4 {
    [v[3], v[0], v[1], v[2]]
}
fn zwxy(v: V4) -> V4 {
    [v[2], v[3], v[0], v[1]]
}

fn ch(p: u32, shift: u32) -> f32 {
    ((p >> shift) & 255) as f32 / 255.0
}

/// The compuphase colour metric, with a change in opacity counting as a full difference.
fn colour_distance(a: u32, b: u32) -> f32 {
    if a == b {
        return 0.0;
    }
    let r = 0.5 * (ch(a, 0) + ch(b, 0));
    let dr = ch(a, 0) - ch(b, 0);
    let dg = ch(a, 8) - ch(b, 8);
    let db = ch(a, 16) - ch(b, 16);
    let colour = ((2.0 + r) * dr * dr + 4.0 * dg * dg + (3.0 - r) * db * db).sqrt() / 3.0;
    colour.max((ch(a, 24) - ch(b, 24)).abs())
}

fn strength(d: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let diff = a[0] - a[1];
    let w1 = (THRESHOLD - d).max(0.0) / THRESHOLD;
    let w2 = (1.0 - d + if a[0].min(b[0]) + a[0] > a[1].min(b[1]) + a[1] { diff } else { -diff }).clamp(0.0, 1.0);
    w1 * w2 * (a[0] * a[1])
}

fn dominance(x: [f32; 3], y: [f32; 3], z: [f32; 3], w: [f32; 3]) -> V4 {
    [2.0 * x[1] - (x[0] + x[2]), 2.0 * y[1] - (y[0] + y[2]), 2.0 * z[1] - (z[0] + z[2]), 2.0 * w[1] - (w[0] + w[2])]
}

fn junction(d: V4) -> V4 {
    let (a, b, c) = (yzwx(d), wxyz(d), zwxy(d));
    std::array::from_fn(|i| (gt(d[i], 0.0) * (le(a[i], 0.0) * le(b[i], 0.0) + gt(d[i] + c[i], a[i] + b[i]))).min(1.0))
}

fn clear(corner: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    (corner[0] >= a[0].min(a[1]).max(b[0].min(b[1])) && corner[1] >= a[0].min(b[1]).max(b[0].min(a[1]))) as u8 as f32
}

#[derive(Clone, Copy, Default)]
struct Tags {
    c: B4,
    h: B4,
    v: B4,
    o: B4,
}

/// ScaleFX: pixel art three times larger, with edges up to level 6 smoothed; only colours of the
/// source are used. Pixels are little-endian RGBA words.
pub fn scalefx(src: &[u32], w: usize, h: usize) -> Vec<u32> {
    let index = |x: isize, y: isize| (y.clamp(0, h as isize - 1) as usize) * w + x.clamp(0, w as isize - 1) as usize;
    let pixel = |x: isize, y: isize| src[index(x, y)];
    let mut metric = vec![[0f32; 4]; w * h];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let e = pixel(x, y);
            metric[y as usize * w + x as usize] = [colour_distance(e, pixel(x - 1, y - 1)), colour_distance(e, pixel(x, y - 1)), colour_distance(e, pixel(x + 1, y - 1)), colour_distance(e, pixel(x + 1, y))];
        }
    }
    let m = |x: isize, y: isize| metric[index(x, y)];
    let mut strengths = vec![[0f32; 4]; w * h];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let (a, b, d, e, f, g, hh, i) = (m(x - 1, y - 1), m(x, y - 1), m(x - 1, y), m(x, y), m(x + 1, y), m(x - 1, y + 1), m(x, y + 1), m(x + 1, y + 1));
            strengths[y as usize * w + x as usize] = [
                strength(d[2], [d[3], e[1]], [a[3], d[1]]),
                strength(f[0], [e[3], e[1]], [b[3], f[1]]),
                strength(hh[2], [e[3], hh[1]], [hh[3], i[1]]),
                strength(hh[0], [d[3], hh[1]], [g[3], g[1]]),
            ];
        }
    }
    let s = |x: isize, y: isize| strengths[index(x, y)];
    let mut tags = vec![Tags::default(); w * h];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let (a, b, d, e, f, g, hh, i) = (m(x - 1, y - 1), m(x, y - 1), m(x - 1, y), m(x, y), m(x + 1, y), m(x - 1, y + 1), m(x, y + 1), m(x + 1, y + 1));
            let (as_, bs, cs, ds, es, fs, gs, hs, is) = (s(x - 1, y - 1), s(x, y - 1), s(x + 1, y - 1), s(x - 1, y), s(x, y), s(x + 1, y), s(x - 1, y + 1), s(x, y + 1), s(x + 1, y + 1));
            let jsx = [as_[2], bs[3], es[0], ds[1]];
            let jsy = [bs[2], cs[3], fs[0], es[1]];
            let jsz = [es[2], fs[3], is[0], hs[1]];
            let jsw = [ds[2], es[3], hs[0], gs[1]];
            let jx = junction(dominance([as_[1], as_[2], as_[3]], [bs[2], bs[3], bs[0]], [es[3], es[0], es[1]], [ds[0], ds[1], ds[2]]));
            let jy = junction(dominance([bs[1], bs[2], bs[3]], [cs[2], cs[3], cs[0]], [fs[3], fs[0], fs[1]], [es[0], es[1], es[2]]));
            let jz = junction(dominance([es[1], es[2], es[3]], [fs[2], fs[3], fs[0]], [is[3], is[0], is[1]], [hs[0], hs[1], hs[2]]));
            let jw = junction(dominance([ds[1], ds[2], ds[3]], [es[2], es[3], es[0]], [hs[3], hs[0], hs[1]], [gs[0], gs[1], gs[2]]));
            let mut res: V4 = [
                (jx[2] + not(jx[1]) * not(jx[3]) * gt(jsx[2], 0.0) * (jx[0] + gt(jsx[0] + jsx[2], jsx[1] + jsx[3]))).min(1.0),
                (jy[3] + not(jy[2]) * not(jy[0]) * gt(jsy[3], 0.0) * (jy[1] + gt(jsy[1] + jsy[3], jsy[0] + jsy[2]))).min(1.0),
                (jz[0] + not(jz[3]) * not(jz[1]) * gt(jsz[0], 0.0) * (jz[2] + gt(jsz[0] + jsz[2], jsz[1] + jsz[3]))).min(1.0),
                (jw[1] + not(jw[0]) * not(jw[2]) * gt(jsw[1], 0.0) * (jw[3] + gt(jsw[1] + jsw[3], jsw[0] + jsw[2]))).min(1.0),
            ];
            let ends = [jx[2], jy[3], jz[0], jw[1]];
            let (before, after) = (wxyz(res), yzwx(res));
            res = std::array::from_fn(|k| (res[k] * (ends[k] + not(before[k] * after[k]))).min(1.0));
            let clr = [
                clear([d[2], e[0]], [d[3], e[1]], [a[3], d[1]]),
                clear([f[0], e[2]], [e[3], e[1]], [b[3], f[1]]),
                clear([hh[2], i[0]], [e[3], hh[1]], [hh[3], i[1]]),
                clear([hh[0], g[2]], [d[3], hh[1]], [g[3], g[1]]),
            ];
            let hm = [d[3].min(a[3]), e[3].min(b[3]), e[3].min(hh[3]), d[3].min(g[3])];
            let vm = [e[1].min(d[1]), e[1].min(f[1]), hh[1].min(i[1]), hh[1].min(g[1])];
            let hw = [d[3], e[3], e[3], d[3]];
            let vw = [e[1], e[1], hh[1], hh[1]];
            tags[y as usize * w + x as usize] = Tags {
                c: std::array::from_fn(|k| res[k] > 0.5),
                h: std::array::from_fn(|k| lt(hm[k], vm[k]) * clr[k] > 0.0),
                v: std::array::from_fn(|k| gt(hm[k], vm[k]) * clr[k] > 0.0),
                o: std::array::from_fn(|k| hm[k] + hw[k] > vm[k] + vw[k]),
            };
        }
    }
    let t = |x: isize, y: isize| tags[index(x, y)];
    const OFFSETS: [(isize, isize); 9] = [(0, 0), (-1, 0), (-2, 0), (1, 0), (2, 0), (0, -1), (0, -2), (0, 1), (0, 2)];
    let sc = SCALEFX_SCALE as usize;
    let ow = w * sc;
    let mut out = vec![0u32; ow * h * sc];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let e = t(x, y);
            let (d, d0, d1) = (t(x - 1, y), t(x - 2, y), t(x - 3, y));
            let (f, f0, f1) = (t(x + 1, y), t(x + 2, y), t(x + 3, y));
            let (b, b0, b1) = (t(x, y - 1), t(x, y - 2), t(x, y - 3));
            let (hh, h0, h1) = (t(x, y + 1), t(x, y + 2), t(x, y + 3));
            let (ec, eh, ev, eo) = (e.c, e.h, e.v, e.o);
            let (dc, dh, do_) = (d.c, d.h, d.o);
            let (d0c, d0h, d1h) = (d0.c, d0.h, d1.h);
            let (fc, fh, fo) = (f.c, f.h, f.o);
            let (f0c, f0h, f1h) = (f0.c, f0.h, f1.h);
            let (bc, bv, bo) = (b.c, b.v, b.o);
            let (b0c, b0v, b1v) = (b0.c, b0.v, b1.v);
            let (hc, hv, ho) = (hh.c, hh.v, hh.o);
            let (h0c, h0v, h1v) = (h0.c, h0.v, h1.v);

            let (l1x, l1y, l1z, l1w) = (ec[0], ec[1], ec[2], ec[3]);
            let l2x = [ec[0] && eh[1] && dc[2], ec[1] && eh[0] && fc[3]];
            let l2y = [ec[1] && ev[2] && bc[3], ec[2] && ev[1] && hc[0]];
            let l2z = [ec[3] && eh[2] && dc[1], ec[2] && eh[3] && fc[0]];
            let l2w = [ec[0] && ev[3] && bc[2], ec[3] && ev[0] && hc[1]];
            let l3x = [l2x[1] && dh[1] && dh[0] && fh[2], l2w[1] && bv[3] && bv[0] && hv[2]];
            let l3y = [l2x[0] && fh[0] && fh[1] && dh[3], l2y[1] && bv[2] && bv[1] && hv[3]];
            let l3z = [l2z[0] && fh[3] && fh[2] && dh[0], l2y[0] && hv[1] && hv[2] && bv[0]];
            let l3w = [l2z[1] && dh[2] && dh[3] && fh[1], l2w[0] && hv[0] && hv[3] && bv[1]];
            let l4x = [dc[0] && dh[1] && eh[0] && eh[1] && fh[0] && fh[1] && d0c[2] && d0h[3], bc[0] && bv[3] && ev[0] && ev[3] && hv[0] && hv[3] && b0c[2] && b0v[1]];
            let l4y = [fc[1] && fh[0] && eh[1] && eh[0] && dh[1] && dh[0] && f0c[3] && f0h[2], bc[1] && bv[2] && ev[1] && ev[2] && hv[1] && hv[2] && b0c[3] && b0v[0]];
            let l4z = [fc[2] && fh[3] && eh[2] && eh[3] && dh[2] && dh[3] && f0c[0] && f0h[1], hc[2] && hv[1] && ev[2] && ev[1] && bv[2] && bv[1] && h0c[0] && h0v[3]];
            let l4w = [dc[3] && dh[2] && eh[3] && eh[2] && fh[3] && fh[2] && d0c[1] && d0h[0], hc[3] && hv[0] && ev[3] && ev[0] && bv[3] && bv[0] && h0c[1] && h0v[2]];
            let l5x = [l4x[0] && f0h[0] && f0h[1] && d1h[2] && d1h[3], l4y[0] && d0h[1] && d0h[0] && f1h[3] && f1h[2]];
            let l5y = [l4y[1] && h0v[1] && h0v[2] && b1v[3] && b1v[0], l4z[1] && b0v[2] && b0v[1] && h1v[0] && h1v[3]];
            let l5z = [l4w[0] && f0h[3] && f0h[2] && d1h[1] && d1h[0], l4z[0] && d0h[2] && d0h[3] && f1h[0] && f1h[1]];
            let l5w = [l4x[1] && h0v[0] && h0v[3] && b1v[2] && b1v[1], l4w[1] && b0v[3] && b0v[0] && h1v[1] && h1v[2]];
            let l6x = [l5x[1] && d1h[1] && d1h[0], l5w[1] && b1v[3] && b1v[0]];
            let l6y = [l5x[0] && f1h[0] && f1h[1], l5y[1] && b1v[2] && b1v[1]];
            let l6z = [l5z[0] && f1h[3] && f1h[2], l5y[0] && h1v[1] && h1v[2]];
            let l6w = [l5z[1] && d1h[2] && d1h[3], l5w[0] && h1v[0] && h1v[3]];

            let pick = |opts: &[(bool, usize)]| opts.iter().find(|(c, _)| *c).map_or(0, |(_, v)| *v);
            let crn = [
                pick(&[((l1x && eo[0]) || (l3x[0] && eo[1]) || (l4x[0] && do_[0]) || (l6x[0] && fo[1]), 5), (l1x || (l3x[1] && !eo[3]) || (l4x[1] && !bo[0]) || (l6x[1] && !ho[3]), 1), (l3x[0], 3), (l3x[1], 7), (l4x[0], 2), (l4x[1], 6), (l6x[0], 4), (l6x[1], 8)]),
                pick(&[((l1y && eo[1]) || (l3y[0] && eo[0]) || (l4y[0] && fo[1]) || (l6y[0] && do_[0]), 5), (l1y || (l3y[1] && !eo[2]) || (l4y[1] && !bo[1]) || (l6y[1] && !ho[2]), 3), (l3y[0], 1), (l3y[1], 7), (l4y[0], 4), (l4y[1], 6), (l6y[0], 2), (l6y[1], 8)]),
                pick(&[((l1z && eo[2]) || (l3z[0] && eo[3]) || (l4z[0] && fo[2]) || (l6z[0] && do_[3]), 7), (l1z || (l3z[1] && !eo[1]) || (l4z[1] && !ho[2]) || (l6z[1] && !bo[1]), 3), (l3z[0], 1), (l3z[1], 5), (l4z[0], 4), (l4z[1], 8), (l6z[0], 2), (l6z[1], 6)]),
                pick(&[((l1w && eo[3]) || (l3w[0] && eo[2]) || (l4w[0] && do_[3]) || (l6w[0] && fo[2]), 7), (l1w || (l3w[1] && !eo[0]) || (l4w[1] && !ho[3]) || (l6w[1] && !bo[0]), 1), (l3w[0], 3), (l3w[1], 5), (l4w[0], 2), (l4w[1], 8), (l6w[0], 4), (l6w[1], 6)]),
            ];
            let mid = [
                pick(&[((l2x[0] && eo[0]) || (l2x[1] && eo[1]) || (l5x[0] && do_[0]) || (l5x[1] && fo[1]), 5), (l2x[0], 1), (l2x[1], 3), (l5x[0], 2), (l5x[1], 4), (ec[0] && dc[2] && ec[1] && fc[3], if eo[0] { if eo[1] { 5 } else { 3 } } else { 1 })]),
                pick(&[((l2y[0] && !eo[1]) || (l2y[1] && !eo[2]) || (l5y[0] && !bo[1]) || (l5y[1] && !ho[2]), 3), (l2y[0], 5), (l2y[1], 7), (l5y[0], 6), (l5y[1], 8), (ec[1] && bc[3] && ec[2] && hc[0], if !eo[1] { if !eo[2] { 3 } else { 7 } } else { 5 })]),
                pick(&[((l2z[0] && eo[3]) || (l2z[1] && eo[2]) || (l5z[0] && do_[3]) || (l5z[1] && fo[2]), 7), (l2z[0], 1), (l2z[1], 3), (l5z[0], 2), (l5z[1], 4), (ec[2] && fc[0] && ec[3] && dc[1], if eo[2] { if eo[3] { 7 } else { 1 } } else { 3 })]),
                pick(&[((l2w[0] && !eo[0]) || (l2w[1] && !eo[3]) || (l5w[0] && !bo[0]) || (l5w[1] && !ho[3]), 1), (l2w[0], 5), (l2w[1], 7), (l5w[0], 6), (l5w[1], 8), (ec[3] && hc[1] && ec[0] && bc[2], if !eo[3] { if !eo[0] { 1 } else { 5 } } else { 7 })]),
            ];
            let grid = [[crn[0], mid[0], crn[1]], [mid[3], 0, mid[1]], [crn[3], mid[2], crn[2]]];
            for sy in 0..sc {
                for sx in 0..sc {
                    let (ox, oy) = OFFSETS[grid[sy][sx]];
                    out[(y as usize * sc + sy) * ow + x as usize * sc + sx] = pixel(x + ox, y + oy);
                }
            }
        }
    }
    out
}
