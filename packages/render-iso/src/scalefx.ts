/*
  A port of ScaleFX (passes 0 to 4) by Sp00kyFox, from libretro/slang-shaders, to run once per tile on the CPU.

  Copyright (c) 2016 Sp00kyFox - ScaleFX@web.de

  Permission is hereby granted, free of charge, to any person obtaining a copy
  of this software and associated documentation files (the "Software"), to deal
  in the Software without restriction, including without limitation the rights
  to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
  copies of the Software, and to permit persons to whom the Software is
  furnished to do so, subject to the following conditions:

  The above copyright notice and this permission notice shall be included in
  all copies or substantial portions of the Software.

  THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
  IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
  FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
  AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
  LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
  OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
  THE SOFTWARE.
*/

type Vec4 = [number, number, number, number]
type Bool4 = [boolean, boolean, boolean, boolean]

export const SCALEFX_SCALE = 3

const THRESHOLD = 0.5
const NO_CORNERS: Bool4 = [false, false, false, false]

const gt = (a: number, b: number) => (a > b ? 1 : 0)
const lt = (a: number, b: number) => (a < b ? 1 : 0)
const le = (a: number, b: number) => (a <= b ? 1 : 0)
const not = (a: number) => 1 - a
const each = (f: (i: number) => number): Vec4 => [f(0), f(1), f(2), f(3)]
const eachBool = (f: (i: number) => boolean): Bool4 => [f(0), f(1), f(2), f(3)]
const yzwx = (v: Vec4): Vec4 => [v[1], v[2], v[3], v[0]]
const wxyz = (v: Vec4): Vec4 => [v[3], v[0], v[1], v[2]]
const zwxy = (v: Vec4): Vec4 => [v[2], v[3], v[0], v[1]]

function channel(pixel: number, shift: number) {
  return ((pixel >>> shift) & 255) / 255
}

/** The compuphase colour metric, with a change in opacity counting as a full difference. */
function colourDistance(a: number, b: number): number {
  if (a === b) return 0
  const r = 0.5 * (channel(a, 0) + channel(b, 0))
  const dr = channel(a, 0) - channel(b, 0)
  const dg = channel(a, 8) - channel(b, 8)
  const db = channel(a, 16) - channel(b, 16)
  const colour = Math.sqrt((2 + r) * dr * dr + 4 * dg * dg + (3 - r) * db * db) / 3
  return Math.max(colour, Math.abs(channel(a, 24) - channel(b, 24)))
}

function strength(d: number, a: [number, number], b: [number, number]): number {
  const diff = a[0] - a[1]
  const weight1 = Math.max(THRESHOLD - d, 0) / THRESHOLD
  const weight2 = Math.min(1, Math.max(0, 1 - d + (Math.min(a[0], b[0]) + a[0] > Math.min(a[1], b[1]) + a[1] ? diff : -diff)))
  return weight1 * weight2 * (a[0] * a[1])
}

function dominance(x: [number, number, number], y: [number, number, number], z: [number, number, number], w: [number, number, number]): Vec4 {
  return [2 * x[1] - (x[0] + x[2]), 2 * y[1] - (y[0] + y[2]), 2 * z[1] - (z[0] + z[2]), 2 * w[1] - (w[0] + w[2])]
}

function junction(d: Vec4): Vec4 {
  const a = yzwx(d)
  const b = wxyz(d)
  const c = zwxy(d)
  return each((i) => Math.min(gt(d[i], 0) * (le(a[i], 0) * le(b[i], 0) + gt(d[i] + c[i], a[i] + b[i])), 1))
}

function clear(corner: [number, number], a: [number, number], b: [number, number]): number {
  return corner[0] >= Math.max(Math.min(a[0], a[1]), Math.min(b[0], b[1])) && corner[1] >= Math.max(Math.min(a[0], b[1]), Math.min(b[0], a[1])) ? 1 : 0
}

interface Tags {
  corners: Bool4
  horizontal: Bool4
  vertical: Bool4
  orientation: Bool4
}

/** ScaleFX: pixel art three times larger, with edges up to level 6 smoothed; only colours of the source are used. */
export function scalefx(src: Uint32Array, w: number, h: number): Uint32Array<ArrayBuffer> {
  const index = (x: number, y: number) => Math.min(h - 1, Math.max(0, y)) * w + Math.min(w - 1, Math.max(0, x))
  const pixel = (x: number, y: number) => src[index(x, y)]

  const metric: Vec4[] = new Array(w * h)
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const e = pixel(x, y)
      metric[y * w + x] = [colourDistance(e, pixel(x - 1, y - 1)), colourDistance(e, pixel(x, y - 1)), colourDistance(e, pixel(x + 1, y - 1)), colourDistance(e, pixel(x + 1, y))]
    }
  const m = (x: number, y: number) => metric[index(x, y)]

  const strengths: Vec4[] = new Array(w * h)
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const [A, B, D, E, F, G, H, I] = [m(x - 1, y - 1), m(x, y - 1), m(x - 1, y), m(x, y), m(x + 1, y), m(x - 1, y + 1), m(x, y + 1), m(x + 1, y + 1)]
      strengths[y * w + x] = [
        strength(D[2], [D[3], E[1]], [A[3], D[1]]),
        strength(F[0], [E[3], E[1]], [B[3], F[1]]),
        strength(H[2], [E[3], H[1]], [H[3], I[1]]),
        strength(H[0], [D[3], H[1]], [G[3], G[1]]),
      ]
    }
  const s = (x: number, y: number) => strengths[index(x, y)]

  const tags: Tags[] = new Array(w * h)
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const [A, B, D, E, F, G, H, I] = [m(x - 1, y - 1), m(x, y - 1), m(x - 1, y), m(x, y), m(x + 1, y), m(x - 1, y + 1), m(x, y + 1), m(x + 1, y + 1)]
      const [As, Bs, Cs, Ds, Es, Fs, Gs, Hs, Is] = [s(x - 1, y - 1), s(x, y - 1), s(x + 1, y - 1), s(x - 1, y), s(x, y), s(x + 1, y), s(x - 1, y + 1), s(x, y + 1), s(x + 1, y + 1)]
      const jSx: Vec4 = [As[2], Bs[3], Es[0], Ds[1]]
      const jSy: Vec4 = [Bs[2], Cs[3], Fs[0], Es[1]]
      const jSz: Vec4 = [Es[2], Fs[3], Is[0], Hs[1]]
      const jSw: Vec4 = [Ds[2], Es[3], Hs[0], Gs[1]]
      const jx = junction(dominance([As[1], As[2], As[3]], [Bs[2], Bs[3], Bs[0]], [Es[3], Es[0], Es[1]], [Ds[0], Ds[1], Ds[2]]))
      const jy = junction(dominance([Bs[1], Bs[2], Bs[3]], [Cs[2], Cs[3], Cs[0]], [Fs[3], Fs[0], Fs[1]], [Es[0], Es[1], Es[2]]))
      const jz = junction(dominance([Es[1], Es[2], Es[3]], [Fs[2], Fs[3], Fs[0]], [Is[3], Is[0], Is[1]], [Hs[0], Hs[1], Hs[2]]))
      const jw = junction(dominance([Ds[1], Ds[2], Ds[3]], [Es[2], Es[3], Es[0]], [Hs[3], Hs[0], Hs[1]], [Gs[0], Gs[1], Gs[2]]))
      let res: Vec4 = [
        Math.min(jx[2] + not(jx[1]) * not(jx[3]) * gt(jSx[2], 0) * (jx[0] + gt(jSx[0] + jSx[2], jSx[1] + jSx[3])), 1),
        Math.min(jy[3] + not(jy[2]) * not(jy[0]) * gt(jSy[3], 0) * (jy[1] + gt(jSy[1] + jSy[3], jSy[0] + jSy[2])), 1),
        Math.min(jz[0] + not(jz[3]) * not(jz[1]) * gt(jSz[0], 0) * (jz[2] + gt(jSz[0] + jSz[2], jSz[1] + jSz[3])), 1),
        Math.min(jw[1] + not(jw[0]) * not(jw[2]) * gt(jSw[1], 0) * (jw[3] + gt(jSw[1] + jSw[3], jSw[0] + jSw[2])), 1),
      ]
      const ends: Vec4 = [jx[2], jy[3], jz[0], jw[1]]
      const before = wxyz(res)
      const after = yzwx(res)
      res = each((i) => Math.min(res[i] * (ends[i] + not(before[i] * after[i])), 1))
      const clr: Vec4 = [
        clear([D[2], E[0]], [D[3], E[1]], [A[3], D[1]]),
        clear([F[0], E[2]], [E[3], E[1]], [B[3], F[1]]),
        clear([H[2], I[0]], [E[3], H[1]], [H[3], I[1]]),
        clear([H[0], G[2]], [D[3], H[1]], [G[3], G[1]]),
      ]
      const hm: Vec4 = [Math.min(D[3], A[3]), Math.min(E[3], B[3]), Math.min(E[3], H[3]), Math.min(D[3], G[3])]
      const vm: Vec4 = [Math.min(E[1], D[1]), Math.min(E[1], F[1]), Math.min(H[1], I[1]), Math.min(H[1], G[1])]
      const hw: Vec4 = [D[3], E[3], E[3], D[3]]
      const vw: Vec4 = [E[1], E[1], H[1], H[1]]
      tags[y * w + x] = {
        corners: eachBool((i) => res[i] > 0.5),
        horizontal: eachBool((i) => lt(hm[i], vm[i]) * clr[i] > 0),
        vertical: eachBool((i) => gt(hm[i], vm[i]) * clr[i] > 0),
        orientation: eachBool((i) => hm[i] + hw[i] > vm[i] + vw[i]),
      }
    }
  const t = (x: number, y: number) => tags[index(x, y)] ?? { corners: NO_CORNERS, horizontal: NO_CORNERS, vertical: NO_CORNERS, orientation: NO_CORNERS }

  const OFFSETS: readonly [number, number][] = [[0, 0], [-1, 0], [-2, 0], [1, 0], [2, 0], [0, -1], [0, -2], [0, 1], [0, 2]]
  const ow = w * SCALEFX_SCALE
  const out = new Uint32Array(ow * h * SCALEFX_SCALE)
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const E = t(x, y)
      const D = t(x - 1, y), D0 = t(x - 2, y), D1 = t(x - 3, y)
      const F = t(x + 1, y), F0 = t(x + 2, y), F1 = t(x + 3, y)
      const B = t(x, y - 1), B0 = t(x, y - 2), B1 = t(x, y - 3)
      const H = t(x, y + 1), H0 = t(x, y + 2), H1 = t(x, y + 3)
      const [Ec, Eh, Ev, Eo] = [E.corners, E.horizontal, E.vertical, E.orientation]
      const [Dc, Dh, Do] = [D.corners, D.horizontal, D.orientation]
      const [D0c, D0h, D1h] = [D0.corners, D0.horizontal, D1.horizontal]
      const [Fc, Fh, Fo] = [F.corners, F.horizontal, F.orientation]
      const [F0c, F0h, F1h] = [F0.corners, F0.horizontal, F1.horizontal]
      const [Bc, Bv, Bo] = [B.corners, B.vertical, B.orientation]
      const [B0c, B0v, B1v] = [B0.corners, B0.vertical, B1.vertical]
      const [Hc, Hv, Ho] = [H.corners, H.vertical, H.orientation]
      const [H0c, H0v, H1v] = [H0.corners, H0.vertical, H1.vertical]

      const lvl1x = Ec[0]
      const lvl1y = Ec[1]
      const lvl1z = Ec[2]
      const lvl1w = Ec[3]
      const lvl2x = [Ec[0] && Eh[1] && Dc[2], Ec[1] && Eh[0] && Fc[3]]
      const lvl2y = [Ec[1] && Ev[2] && Bc[3], Ec[2] && Ev[1] && Hc[0]]
      const lvl2z = [Ec[3] && Eh[2] && Dc[1], Ec[2] && Eh[3] && Fc[0]]
      const lvl2w = [Ec[0] && Ev[3] && Bc[2], Ec[3] && Ev[0] && Hc[1]]
      const lvl3x = [lvl2x[1] && Dh[1] && Dh[0] && Fh[2], lvl2w[1] && Bv[3] && Bv[0] && Hv[2]]
      const lvl3y = [lvl2x[0] && Fh[0] && Fh[1] && Dh[3], lvl2y[1] && Bv[2] && Bv[1] && Hv[3]]
      const lvl3z = [lvl2z[0] && Fh[3] && Fh[2] && Dh[0], lvl2y[0] && Hv[1] && Hv[2] && Bv[0]]
      const lvl3w = [lvl2z[1] && Dh[2] && Dh[3] && Fh[1], lvl2w[0] && Hv[0] && Hv[3] && Bv[1]]
      const lvl4x = [Dc[0] && Dh[1] && Eh[0] && Eh[1] && Fh[0] && Fh[1] && D0c[2] && D0h[3], Bc[0] && Bv[3] && Ev[0] && Ev[3] && Hv[0] && Hv[3] && B0c[2] && B0v[1]]
      const lvl4y = [Fc[1] && Fh[0] && Eh[1] && Eh[0] && Dh[1] && Dh[0] && F0c[3] && F0h[2], Bc[1] && Bv[2] && Ev[1] && Ev[2] && Hv[1] && Hv[2] && B0c[3] && B0v[0]]
      const lvl4z = [Fc[2] && Fh[3] && Eh[2] && Eh[3] && Dh[2] && Dh[3] && F0c[0] && F0h[1], Hc[2] && Hv[1] && Ev[2] && Ev[1] && Bv[2] && Bv[1] && H0c[0] && H0v[3]]
      const lvl4w = [Dc[3] && Dh[2] && Eh[3] && Eh[2] && Fh[3] && Fh[2] && D0c[1] && D0h[0], Hc[3] && Hv[0] && Ev[3] && Ev[0] && Bv[3] && Bv[0] && H0c[1] && H0v[2]]
      const lvl5x = [lvl4x[0] && F0h[0] && F0h[1] && D1h[2] && D1h[3], lvl4y[0] && D0h[1] && D0h[0] && F1h[3] && F1h[2]]
      const lvl5y = [lvl4y[1] && H0v[1] && H0v[2] && B1v[3] && B1v[0], lvl4z[1] && B0v[2] && B0v[1] && H1v[0] && H1v[3]]
      const lvl5z = [lvl4w[0] && F0h[3] && F0h[2] && D1h[1] && D1h[0], lvl4z[0] && D0h[2] && D0h[3] && F1h[0] && F1h[1]]
      const lvl5w = [lvl4x[1] && H0v[0] && H0v[3] && B1v[2] && B1v[1], lvl4w[1] && B0v[3] && B0v[0] && H1v[1] && H1v[2]]
      const lvl6x = [lvl5x[1] && D1h[1] && D1h[0], lvl5w[1] && B1v[3] && B1v[0]]
      const lvl6y = [lvl5x[0] && F1h[0] && F1h[1], lvl5y[1] && B1v[2] && B1v[1]]
      const lvl6z = [lvl5z[0] && F1h[3] && F1h[2], lvl5y[0] && H1v[1] && H1v[2]]
      const lvl6w = [lvl5z[1] && D1h[2] && D1h[3], lvl5w[0] && H1v[0] && H1v[3]]

      const crn = [
        (lvl1x && Eo[0]) || (lvl3x[0] && Eo[1]) || (lvl4x[0] && Do[0]) || (lvl6x[0] && Fo[1]) ? 5 : lvl1x || (lvl3x[1] && !Eo[3]) || (lvl4x[1] && !Bo[0]) || (lvl6x[1] && !Ho[3]) ? 1 : lvl3x[0] ? 3 : lvl3x[1] ? 7 : lvl4x[0] ? 2 : lvl4x[1] ? 6 : lvl6x[0] ? 4 : lvl6x[1] ? 8 : 0,
        (lvl1y && Eo[1]) || (lvl3y[0] && Eo[0]) || (lvl4y[0] && Fo[1]) || (lvl6y[0] && Do[0]) ? 5 : lvl1y || (lvl3y[1] && !Eo[2]) || (lvl4y[1] && !Bo[1]) || (lvl6y[1] && !Ho[2]) ? 3 : lvl3y[0] ? 1 : lvl3y[1] ? 7 : lvl4y[0] ? 4 : lvl4y[1] ? 6 : lvl6y[0] ? 2 : lvl6y[1] ? 8 : 0,
        (lvl1z && Eo[2]) || (lvl3z[0] && Eo[3]) || (lvl4z[0] && Fo[2]) || (lvl6z[0] && Do[3]) ? 7 : lvl1z || (lvl3z[1] && !Eo[1]) || (lvl4z[1] && !Ho[2]) || (lvl6z[1] && !Bo[1]) ? 3 : lvl3z[0] ? 1 : lvl3z[1] ? 5 : lvl4z[0] ? 4 : lvl4z[1] ? 8 : lvl6z[0] ? 2 : lvl6z[1] ? 6 : 0,
        (lvl1w && Eo[3]) || (lvl3w[0] && Eo[2]) || (lvl4w[0] && Do[3]) || (lvl6w[0] && Fo[2]) ? 7 : lvl1w || (lvl3w[1] && !Eo[0]) || (lvl4w[1] && !Ho[3]) || (lvl6w[1] && !Bo[0]) ? 1 : lvl3w[0] ? 3 : lvl3w[1] ? 5 : lvl4w[0] ? 2 : lvl4w[1] ? 8 : lvl6w[0] ? 4 : lvl6w[1] ? 6 : 0,
      ]
      const mid = [
        (lvl2x[0] && Eo[0]) || (lvl2x[1] && Eo[1]) || (lvl5x[0] && Do[0]) || (lvl5x[1] && Fo[1]) ? 5 : lvl2x[0] ? 1 : lvl2x[1] ? 3 : lvl5x[0] ? 2 : lvl5x[1] ? 4 : Ec[0] && Dc[2] && Ec[1] && Fc[3] ? (Eo[0] ? (Eo[1] ? 5 : 3) : 1) : 0,
        (lvl2y[0] && !Eo[1]) || (lvl2y[1] && !Eo[2]) || (lvl5y[0] && !Bo[1]) || (lvl5y[1] && !Ho[2]) ? 3 : lvl2y[0] ? 5 : lvl2y[1] ? 7 : lvl5y[0] ? 6 : lvl5y[1] ? 8 : Ec[1] && Bc[3] && Ec[2] && Hc[0] ? (!Eo[1] ? (!Eo[2] ? 3 : 7) : 5) : 0,
        (lvl2z[0] && Eo[3]) || (lvl2z[1] && Eo[2]) || (lvl5z[0] && Do[3]) || (lvl5z[1] && Fo[2]) ? 7 : lvl2z[0] ? 1 : lvl2z[1] ? 3 : lvl5z[0] ? 2 : lvl5z[1] ? 4 : Ec[2] && Fc[0] && Ec[3] && Dc[1] ? (Eo[2] ? (Eo[3] ? 7 : 1) : 3) : 0,
        (lvl2w[0] && !Eo[0]) || (lvl2w[1] && !Eo[3]) || (lvl5w[0] && !Bo[0]) || (lvl5w[1] && !Ho[3]) ? 1 : lvl2w[0] ? 5 : lvl2w[1] ? 7 : lvl5w[0] ? 6 : lvl5w[1] ? 8 : Ec[3] && Hc[1] && Ec[0] && Bc[2] ? (!Eo[3] ? (!Eo[0] ? 1 : 5) : 7) : 0,
      ]
      const grid = [
        [crn[0], mid[0], crn[1]],
        [mid[3], 0, mid[1]],
        [crn[3], mid[2], crn[2]],
      ]
      for (let sy = 0; sy < SCALEFX_SCALE; sy++)
        for (let sx = 0; sx < SCALEFX_SCALE; sx++) {
          const [ox, oy] = OFFSETS[grid[sy][sx]]
          out[(y * SCALEFX_SCALE + sy) * ow + x * SCALEFX_SCALE + sx] = pixel(x + ox, y + oy)
        }
    }
  return out
}
