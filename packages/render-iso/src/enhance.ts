import type { TileRect } from '@isorun/scene'
import { SCALEFX_SCALE, scalefx } from './scalefx.js'
import { UNLABELLED, labelOf, tileHash, type Joints, type TileLabel } from './tile-labels.js'

/** Each tile is upscaled four times (by repeating pixels, by Scale2x twice, or by xBR twice), or three times by ScaleFX. */
export const ENHANCE_SCALE = 4

export const UPSCALES = ['pixel', 'scale2x', 'xbr', 'scalefx'] as const
export type Upscale = (typeof UPSCALES)[number]

const DOME_RADIUS = 10
const OUTLINE_TRIM = 2
const OUTLINE_LUMINANCE = 40
const RELIEF = 1.6
/** How strongly the height of a floor or a wall tile bends its normals. */
const TERRAIN_RELIEF = 6
/** How far the height of each relief level reaches from the middle, on the 0 to 1 scale of the parallax. */
const RELIEF_AMPLITUDE = [0.08, 0.35, 0.7, 1]
/** Source texels across a part between the joints: the height is the brightness against the mean over that span. */
const JOINT_SPAN: Record<Joints, number> = { none: 3, bricks: 4, blocks: 6, cobbles: 4, planks: 3, tiles: 5, cracks: 2 }
/** Joints between solid parts: their faces are flat and their edges fall off steeply. */
const SOLID_PARTS: ReadonlySet<Joints> = new Set(['bricks', 'blocks', 'cobbles', 'tiles'])
/** Gloss of each gloss level, and of a wet surface at least. */
const GLOSS = [0, 0.3, 0.6, 1]
const WET_GLOSS = 0.65

export interface PaintSettings {
  /** Kuwahara brush size in upscaled texels: fine noise turns into flat patches with clean edges; 0 for none. */
  radius: number
  /** Tones per colour channel; 0 keeps every tone. */
  levels: number
  /** Brightens the dark tones more than the bright ones (a gamma); 1 keeps the tile as it is. */
  lift: number
  /** How the tiles grow: sharp square pixels, Scale2x (rounded diagonal steps), xBR (smooth anti-aliased edges, as emulators draw pixel art), or ScaleFX (smooth slopes in the colours of the source). */
  upscale: Upscale
}

/** Scale2x (EPX): doubles pixel art, rounding diagonal steps without blurring. */
function scale2x(src: Uint32Array, w: number, h: number): Uint32Array<ArrayBuffer> {
  const out = new Uint32Array(w * h * 4)
  const at = (x: number, y: number) => src[Math.min(h - 1, Math.max(0, y)) * w + Math.min(w - 1, Math.max(0, x))]
  const ow = w * 2
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const p = at(x, y)
      const a = at(x, y - 1)
      const b = at(x + 1, y)
      const c = at(x - 1, y)
      const d = at(x, y + 1)
      const o = y * 2 * ow + x * 2
      out[o] = c === a && c !== d && a !== b ? a : p
      out[o + 1] = a === b && a !== c && b !== d ? b : p
      out[o + ow] = d === c && d !== b && c !== a ? c : p
      out[o + ow + 1] = b === d && b !== a && d !== c ? d : p
    }
  }
  return out
}

/** Repeats every pixel `factor` times across and down. */
function nearestScale(src: Uint32Array, w: number, h: number, factor: number): Uint32Array<ArrayBuffer> {
  const ow = w * factor
  const out = new Uint32Array(ow * h * factor)
  for (let y = 0; y < h * factor; y++)
    for (let x = 0; x < ow; x++) out[y * ow + x] = src[Math.floor(y / factor) * w + Math.floor(x / factor)]
  return out
}

const XBR_CORNERS = [[1, 1], [-1, 1], [-1, -1], [1, -1]] as const

function channel(pixel: number, shift: number) {
  return (pixel >>> shift) & 255
}

/** How different two pixels look: brightness counts most, then colour, and transparency strongly. */
function pixelDistance(a: number, b: number): number {
  if (a === b) return 0
  const dr = channel(a, 0) - channel(b, 0)
  const dg = channel(a, 8) - channel(b, 8)
  const db = channel(a, 16) - channel(b, 16)
  const da = channel(a, 24) - channel(b, 24)
  const y = Math.abs(0.299 * dr + 0.587 * dg + 0.114 * db)
  const u = Math.abs(-0.169 * dr - 0.331 * dg + 0.5 * db)
  const v = Math.abs(0.5 * dr - 0.419 * dg - 0.081 * db)
  return 48 * y + 7 * u + 6 * v + 64 * Math.abs(da)
}

/** The mean of two pixels, weighted by their opacity so transparent pixels add no colour. */
function blendPixels(a: number, b: number): number {
  const aa = channel(a, 24)
  const ba = channel(b, 24)
  const alpha = aa + ba
  if (alpha === 0) return 0
  const mix = (shift: number) => Math.round((channel(a, shift) * aa + channel(b, shift) * ba) / alpha)
  return (mix(0) | (mix(8) << 8) | (mix(16) << 16) | (Math.round(alpha / 2) << 24)) >>> 0
}

/** xBR at 2x: each corner of a pixel takes the colour across an edge when the edge runs along that corner. */
function xbr2x(src: Uint32Array, w: number, h: number): Uint32Array<ArrayBuffer> {
  const out = new Uint32Array(w * h * 4)
  const ow = w * 2
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const e = src[y * w + x]
      for (const [sx, sy] of XBR_CORNERS) {
        const at = (dx: number, dy: number) => {
          const px = Math.min(w - 1, Math.max(0, x + (sx === sy ? dx * sx : dy * sx)))
          const py = Math.min(h - 1, Math.max(0, y + (sx === sy ? dy * sy : dx * sy)))
          return src[py * w + px]
        }
        const f = at(1, 0)
        const hh = at(0, 1)
        let corner = e
        if (e !== f && e !== hh) {
          const i = at(1, 1)
          const edgeAcross = pixelDistance(e, at(1, -1)) + pixelDistance(e, at(-1, 1)) + pixelDistance(i, at(2, 0)) + pixelDistance(i, at(0, 2)) + 4 * pixelDistance(hh, f)
          const edgeAlong = pixelDistance(hh, at(-1, 0)) + pixelDistance(hh, at(1, 2)) + pixelDistance(f, at(2, 1)) + pixelDistance(f, at(0, -1)) + 4 * pixelDistance(e, i)
          if (edgeAcross < edgeAlong) corner = blendPixels(e, pixelDistance(e, f) <= pixelDistance(e, hh) ? f : hh)
        }
        out[(y * 2 + (sy > 0 ? 1 : 0)) * ow + x * 2 + (sx > 0 ? 1 : 0)] = corner
      }
    }
  return out
}

/** How many times larger a way of upscaling makes the tiles. */
export function upscaleFactor(how: Upscale): number {
  return how === 'scalefx' ? SCALEFX_SCALE : ENHANCE_SCALE
}

/** A tile's pixels grown `upscaleFactor` times in the chosen way. */
function upscale(pixels: Uint8ClampedArray, w: number, h: number, how: Upscale): Uint32Array<ArrayBuffer> {
  const data = new Uint32Array(pixels.buffer.slice(0))
  if (how === 'pixel') return nearestScale(data, w, h, ENHANCE_SCALE)
  if (how === 'scalefx') return scalefx(data, w, h)
  if (how === 'xbr') return xbr2x(xbr2x(data, w, h), w * 2, h * 2)
  return scale2x(scale2x(data, w, h), w * 2, h * 2)
}

/** An image grown `upscaleFactor` times in the chosen way, for pixel art shown large outside the dungeon. */
export function upscaleImage(image: ImageData, how: Upscale): ImageData {
  const factor = upscaleFactor(how)
  const grown = upscale(image.data, image.width, image.height, how)
  return new ImageData(new Uint8ClampedArray(grown.buffer), image.width * factor, image.height * factor)
}

/** Distance in pixels to the nearest transparent pixel, capped at `cap` (two-pass chamfer). */
function edgeDistance(alpha: Uint8ClampedArray, w: number, h: number, cap: number): Float32Array {
  const d = new Float32Array(w * h)
  for (let i = 0; i < w * h; i++) d[i] = alpha[i * 4 + 3] < 128 ? 0 : cap
  const diagonal = Math.SQRT2
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const i = y * w + x
      if (d[i] === 0) continue
      if (x > 0) d[i] = Math.min(d[i], d[i - 1] + 1)
      if (y > 0) d[i] = Math.min(d[i], d[i - w] + 1)
      if (x > 0 && y > 0) d[i] = Math.min(d[i], d[i - w - 1] + diagonal)
      if (x < w - 1 && y > 0) d[i] = Math.min(d[i], d[i - w + 1] + diagonal)
    }
  for (let y = h - 1; y >= 0; y--)
    for (let x = w - 1; x >= 0; x--) {
      const i = y * w + x
      if (d[i] === 0) continue
      if (x < w - 1) d[i] = Math.min(d[i], d[i + 1] + 1)
      if (y < h - 1) d[i] = Math.min(d[i], d[i + w] + 1)
      if (x < w - 1 && y < h - 1) d[i] = Math.min(d[i], d[i + w + 1] + diagonal)
      if (x > 0 && y < h - 1) d[i] = Math.min(d[i], d[i + w - 1] + diagonal)
    }
  return d
}

/** DCSS outlines every sprite in one black pixel, four after upscaling; trim the outer half so the line stays fine. */
function trimOutline(pixels: Uint8ClampedArray, w: number, h: number) {
  const distance = edgeDistance(pixels, w, h, OUTLINE_TRIM + 1)
  for (let i = 0; i < w * h; i++) {
    const o = i * 4
    const luminance = 0.299 * pixels[o] + 0.587 * pixels[o + 1] + 0.114 * pixels[o + 2]
    if (pixels[o + 3] >= 128 && distance[i] <= OUTLINE_TRIM && luminance < OUTLINE_LUMINANCE) pixels[o + 3] = 0
  }
}

/**
 * Kuwahara filter on the opaque texels: each texel takes the mean colour of
 * the most even of the four squares that meet at it, so detail flattens into
 * patches and edges stay sharp. Summed-area tables make every square one lookup.
 */
function kuwahara(pixels: Uint8ClampedArray, w: number, h: number, radius: number) {
  const stride = w + 1
  const sums = [new Float64Array(stride * (h + 1)), new Float64Array(stride * (h + 1)), new Float64Array(stride * (h + 1)), new Float64Array(stride * (h + 1)), new Float64Array(stride * (h + 1)), new Float64Array(stride * (h + 1))]
  const [sr, sg, sb, sl, sll, sn] = sums
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * 4
      const opaque = pixels[o + 3] >= 128 ? 1 : 0
      const l = 0.299 * pixels[o] + 0.587 * pixels[o + 1] + 0.114 * pixels[o + 2]
      const values = [pixels[o] * opaque, pixels[o + 1] * opaque, pixels[o + 2] * opaque, l * opaque, l * l * opaque, opaque]
      const i = (y + 1) * stride + x + 1
      sums.forEach((sum, k) => (sum[i] = values[k] + sum[i - 1] + sum[i - stride] - sum[i - stride - 1]))
    }
  const area = (sum: Float64Array, x0: number, y0: number, x1: number, y1: number) => sum[y1 * stride + x1] - sum[y0 * stride + x1] - sum[y1 * stride + x0] + sum[y0 * stride + x0]
  const out = pixels.slice()
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * 4
      if (pixels[o + 3] < 128) continue
      let best = Infinity
      for (const [qx, qy] of [[x - radius, y - radius], [x, y - radius], [x - radius, y], [x, y]]) {
        const x0 = Math.max(0, qx)
        const y0 = Math.max(0, qy)
        const x1 = Math.min(w, qx + radius + 1)
        const y1 = Math.min(h, qy + radius + 1)
        const n = area(sn, x0, y0, x1, y1)
        if (n <= 0) continue
        const mean = area(sl, x0, y0, x1, y1) / n
        const variance = area(sll, x0, y0, x1, y1) / n - mean * mean
        if (variance >= best) continue
        best = variance
        out[o] = area(sr, x0, y0, x1, y1) / n
        out[o + 1] = area(sg, x0, y0, x1, y1) / n
        out[o + 2] = area(sb, x0, y0, x1, y1) / n
      }
    }
  pixels.set(out)
}

function lift(pixels: Uint8ClampedArray, amount: number) {
  const curve = Array.from({ length: 256 }, (_, v) => 255 * Math.pow(v / 255, 1 / amount))
  for (let i = 0; i < pixels.length; i += 4) for (let c = 0; c < 3; c++) pixels[i + c] = curve[pixels[i + c]]
}

function posterize(pixels: Uint8ClampedArray, levels: number) {
  const step = 255 / Math.max(1, levels - 1)
  for (let i = 0; i < pixels.length; i += 4) for (let c = 0; c < 3; c++) pixels[i + c] = Math.round(pixels[i + c] / step) * step
}

/**
 * The height of a sprite: a rounded dome from the silhouette (so a sprite reads as a body the light wraps around)
 * plus its brightness (so folds sit below cloth).
 */
function spriteHeight(pixels: Uint8ClampedArray, w: number, h: number): Float32Array {
  const distance = edgeDistance(pixels, w, h, DOME_RADIUS)
  const height = new Float32Array(w * h)
  for (let i = 0; i < w * h; i++) {
    const t = distance[i] / DOME_RADIUS
    const dome = Math.sqrt(Math.max(0, 1 - (1 - t) * (1 - t)))
    height[i] = dome * 0.4 + luminance(pixels, i) * 0.35
  }
  return height
}

/**
 * The height of a floor or a wall tile, 0 to 1 around 0.5, by its label: the brightness against its mean over the
 * span of one part (a brick, a plank), so the parts stand up from their joints and the noise of single texels and
 * the light of the whole tile flatten out. Solid parts get flat faces and steep edges. The tile wraps, as floors
 * and walls repeat.
 */
function terrainHeight(pixels: Uint8ClampedArray, w: number, h: number, scale: number, label: TileLabel): Float32Array {
  const bright = Float32Array.from({ length: w * h }, (_, i) => (label.brightIsHigh ? luminance(pixels, i) : 1 - luminance(pixels, i)))
  const fine = boxBlur(boxBlur(bright, w, h, Math.max(1, scale >> 1)), w, h, Math.max(1, scale >> 1))
  const span = JOINT_SPAN[label.joints] * scale
  const broad = boxBlur(boxBlur(bright, w, h, span >> 1), w, h, span >> 1)
  const band = fine.map((v, i) => v - broad[i])
  const spread = Math.sqrt(band.reduce((sum, v) => sum + v * v, 0) / band.length) * 2.5 || 1
  const solid = SOLID_PARTS.has(label.joints)
  const amplitude = RELIEF_AMPLITUDE[label.relief] ?? RELIEF_AMPLITUDE[1]
  return band.map((v) => {
    const t = Math.max(-1, Math.min(1, v / spread))
    const shaped = solid ? Math.tanh(t * 2) / Math.tanh(2) : t
    return 0.5 + shaped * 0.5 * amplitude
  })
}

/** A box blur of `radius` texels across and down, wrapping at the edges. */
function boxBlur(values: Float32Array, w: number, h: number, radius: number): Float32Array {
  if (radius < 1) return values
  const across = new Float32Array(w * h)
  const size = radius * 2 + 1
  for (let y = 0; y < h; y++) {
    let sum = 0
    for (let k = -radius; k <= radius; k++) sum += values[y * w + ((k % w) + w) % w]
    for (let x = 0; x < w; x++) {
      across[y * w + x] = sum / size
      sum += values[y * w + ((x + radius + 1) % w)] - values[y * w + (((x - radius) % w) + w) % w]
    }
  }
  const out = new Float32Array(w * h)
  for (let x = 0; x < w; x++) {
    let sum = 0
    for (let k = -radius; k <= radius; k++) sum += across[(((k % h) + h) % h) * w + x]
    for (let y = 0; y < h; y++) {
      out[y * w + x] = sum / size
      sum += across[((y + radius + 1) % h) * w + x] - across[(((y - radius) % h) + h) % h * w + x]
    }
  }
  return out
}

function luminance(pixels: Uint8ClampedArray, i: number) {
  return (0.299 * pixels[i * 4] + 0.587 * pixels[i * 4 + 1] + 0.114 * pixels[i * 4 + 2]) / 255
}

/**
 * A tangent-space normal map from a height, bent by `relief`. The blue channel holds the height for the parallax
 * (`depth`, 0 to 1); the shader rebuilds the normal's z from its x and y.
 */
function normalMap(height: Float32Array, depth: Float32Array, w: number, h: number, relief: number): Uint8ClampedArray<ArrayBuffer> {
  const out = new Uint8ClampedArray(w * h * 4)
  const at = (x: number, y: number) => height[Math.min(h - 1, Math.max(0, y)) * w + Math.min(w - 1, Math.max(0, x))]
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const dx = (at(x + 1, y) - at(x - 1, y)) * relief
      const dy = (at(x, y + 1) - at(x, y - 1)) * relief
      const len = Math.hypot(dx, dy, 1)
      const o = (y * w + x) * 4
      out[o] = ((-dx / len) * 0.5 + 0.5) * 255
      out[o + 1] = ((dy / len) * 0.5 + 0.5) * 255
      out[o + 2] = depth[y * w + x] * 255
      out[o + 3] = 255
    }
  return out
}

/**
 * What each texel's surface does, by the tile's label: red its gloss, green how much light it gives (all of it, or
 * its bright saturated parts: runes, eyes, flames), blue whether it sparkles (its bright texels), alpha whether it is
 * see-through.
 */
function surfaceMap(pixels: Uint8ClampedArray, w: number, h: number, label: TileLabel): Uint8ClampedArray<ArrayBuffer> {
  const out = new Uint8ClampedArray(w * h * 4)
  const gloss = Math.max(GLOSS[label.gloss] ?? 0, label.wet ? WET_GLOSS : 0)
  for (let i = 0; i < w * h; i++) {
    const r = pixels[i * 4] / 255
    const g = pixels[i * 4 + 1] / 255
    const b = pixels[i * 4 + 2] / 255
    const value = Math.max(r, g, b)
    const saturation = value > 0 ? (value - Math.min(r, g, b)) / value : 0
    const light = luminance(pixels, i)
    const glow = label.glow >= 2 ? smoothstep(0.15, 0.7, value) : label.glow === 1 ? smoothstep(0.55, 0.85, value) * smoothstep(0.3, 0.65, saturation) : 0
    out[i * 4] = gloss * (0.75 + 0.5 * (light - 0.5)) * 255
    out[i * 4 + 1] = glow * 255
    out[i * 4 + 2] = label.sparkle ? smoothstep(0.55, 0.9, light) * 255 : 0
    out[i * 4 + 3] = label.translucent ? 255 : 0
  }
  return out
}

function smoothstep(edge0: number, edge1: number, x: number) {
  const t = Math.max(0, Math.min(1, (x - edge0) / (edge1 - edge0)))
  return t * t * (3 - 2 * t)
}

/** The height spread over 0 to 1 between the lowest and the highest opaque texel of the tile; transparent texels are at 0. */
function tileDepth(pixels: Uint8ClampedArray, height: Float32Array): Float32Array {
  const opaque = (i: number) => pixels[i * 4 + 3] >= 128
  let low = Infinity
  let high = -Infinity
  height.forEach((h, i) => {
    if (!opaque(i)) return
    low = Math.min(low, h)
    high = Math.max(high, h)
  })
  const range = high - low
  return height.map((h, i) => (opaque(i) && range > 0 ? (h - low) / range : 0))
}

/** How many source texels across and down one page of upscaled tiles holds. */
export const PAGE_TEXELS = 1024
/** Source texels between tiles on a page, so smooth filtering and mipmaps do not pick up the next tile. */
const PAGE_GUTTER = 2

/** Where a tile stands on a page, in source texels. */
export interface PageSlot {
  sx: number
  sy: number
}

/** A tile just drawn on a page: its slot, and its upscaled pixels with their place on the page's canvases, to upload alone. */
export interface DrawnSlot extends PageSlot {
  x: number
  y: number
  colour: ImageData
  normal: ImageData
  surface: ImageData
  /** What the tile is, by its pixels; null when no label names it. */
  label: TileLabel | null
}

/**
 * A page of tiles upscaled four times (three for ScaleFX), filled one tile at a
 * time as tiles are first drawn, in rows from the top: the colour, and a normal
 * map beside it. Only the tiles in use are upscaled, so a page stays within
 * every GPU's texture size, however big the atlases the tiles come from. The
 * page itself lives on the GPU alone (materials.ts `AtlasMaterials`): each tile
 * is handed over to be copied there.
 */
export class EnhancedPage {
  /** The page's size in texels, as upscaled. */
  readonly size: number
  private readonly scale: number
  private row = { x: 0, y: 0, height: 0 }

  constructor(private readonly paint: PaintSettings) {
    this.scale = upscaleFactor(paint.upscale)
    this.size = PAGE_TEXELS * this.scale
  }

  /** Upscale the tile from its atlas onto the page; null when the page has no room left for it. */
  place(rect: TileRect, atlas: CanvasRenderingContext2D): DrawnSlot | null {
    const slot = this.allocate(rect.w, rect.h)
    if (!slot) return null
    const pixels = atlas.getImageData(rect.sx, rect.sy, rect.w, rect.h)
    const label = labelOf(tileHash(pixels.data, rect.w, rect.h))
    const data = upscale(pixels.data, rect.w, rect.h, this.paint.upscale)
    const w = rect.w * this.scale
    const h = rect.h * this.scale
    const colour = new Uint8ClampedArray(data.buffer)
    const sprite = rect.ox !== 0 || rect.oy !== 0 || rect.w !== rect.cell || rect.h !== rect.cell || hasTransparency(colour)
    if (sprite) trimOutline(colour, w, h)
    if (this.paint.lift !== 1) lift(colour, this.paint.lift)
    if (this.paint.radius > 0) kuwahara(colour, w, h, Math.round(this.paint.radius))
    if (this.paint.levels > 1) posterize(colour, Math.round(this.paint.levels))
    const surface = surfaceMap(colour, w, h, label ?? UNLABELLED)
    let normal: Uint8ClampedArray<ArrayBuffer>
    if (sprite) {
      const height = spriteHeight(colour, w, h)
      normal = normalMap(height, tileDepth(colour, height), w, h, RELIEF)
    } else {
      const height = terrainHeight(colour, w, h, this.scale, label ?? UNLABELLED)
      normal = normalMap(height, height, w, h, TERRAIN_RELIEF)
    }
    return { ...slot, x: slot.sx * this.scale, y: slot.sy * this.scale, colour: new ImageData(colour, w, h), normal: new ImageData(normal, w, h), surface: new ImageData(surface, w, h), label }
  }

  private allocate(w: number, h: number): PageSlot | null {
    if (w + PAGE_GUTTER > PAGE_TEXELS || h + PAGE_GUTTER > PAGE_TEXELS) return null
    if (this.row.x + w + PAGE_GUTTER > PAGE_TEXELS) this.row = { x: 0, y: this.row.y + this.row.height, height: 0 }
    if (this.row.y + h + PAGE_GUTTER > PAGE_TEXELS) return null
    const slot = { sx: this.row.x + PAGE_GUTTER / 2, sy: this.row.y + PAGE_GUTTER / 2 }
    this.row = { x: this.row.x + w + PAGE_GUTTER, y: this.row.y, height: Math.max(this.row.height, h + PAGE_GUTTER) }
    return slot
  }
}

function hasTransparency(pixels: Uint8ClampedArray) {
  for (let i = 3; i < pixels.length; i += 4) if (pixels[i] < 128) return true
  return false
}

