import * as THREE from 'three/webgpu'
import type { TileRect, TileSource } from '@isorun/scene'
import { EnhancedPage, PAGE_TEXELS, type DrawnSlot, type PaintSettings } from './enhance.js'
import { ClayMaterial, actorMaterial, dressGround, dressSprite, type MemoryMask } from './shading.js'
import type { TileLabel } from './tile-labels.js'
import { atlasImage } from './plan.js'

export type TileMaterial = ClayMaterial

export interface AtlasMaterial {
  material: TileMaterial
  sprite: TileMaterial
  /** The sprite material of the actors, whose flash and fade are per mesh (shading.ts `setActorLook`). */
  actor: TileMaterial
  width: number
  height: number
}

/** A tile on its page: the page's materials, the tile's rect with the page as its atlas and its place there, and what the tile is. */
export interface PlacedTile extends AtlasMaterial {
  rect: TileRect
  label: TileLabel | null
}

interface Page {
  enhanced: EnhancedPage
  colour: THREE.DataTexture
  normal: THREE.DataTexture
  surface: THREE.DataTexture
  material: AtlasMaterial
}

/** Something that copies a texture into a region of another on the GPU (three's `Renderer.copyTextureToTexture`). */
export interface TextureCopier {
  copyTextureToTexture(src: THREE.Texture, dst: THREE.Texture, srcRegion?: THREE.Box2 | null, dstPosition?: THREE.Vector2 | null): void
}

/** The tiles in use, upscaled onto pages (enhance.ts `EnhancedPage`) with their normal maps, and one material per page: tiles at four times their size, with relief lighting. */
export class AtlasMaterials {
  private readonly pages: Page[] = []
  private readonly placed = new Map<string, PlacedTile>()
  private readonly atlases = new Map<string, CanvasRenderingContext2D | null>()
  private readonly averages = new Map<string, [number, number, number]>()
  /** Tiles drawn on a page since the last upload, uploaded alone: a page is too big to upload again for each tile. */
  private readonly drawn: { page: Page; slot: DrawnSlot }[] = []
  /** Whether a tile that gives light was placed: the screen then blooms the light it gives. */
  glowing = false

  constructor(
    private readonly tiles: TileSource,
    private readonly paint: PaintSettings,
    private readonly memory: MemoryMask,
  ) {}

  /** The mean colour of a tile's opaque texels, 0..1 in linear light. */
  averageColour(rect: TileRect): [number, number, number] {
    const key = `${rect.atlas}|${rect.sx},${rect.sy},${rect.w},${rect.h}`
    const cached = this.averages.get(key)
    if (cached) return cached
    const colour = this.measureColour(rect)
    this.averages.set(key, colour)
    return colour
  }

  private measureColour(rect: TileRect): [number, number, number] {
    const ctx = this.atlas(rect.atlas)
    if (!ctx) return [0.3, 0.3, 0.3]
    const data = ctx.getImageData(rect.sx, rect.sy, rect.w, rect.h).data
    let r = 0
    let g = 0
    let b = 0
    let n = 0
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] < 128) continue
      r += data[i]
      g += data[i + 1]
      b += data[i + 2]
      n++
    }
    const colour = new THREE.Color().setRGB(r / Math.max(1, n) / 255, g / Math.max(1, n) / 255, b / Math.max(1, n) / 255, THREE.SRGBColorSpace)
    return [colour.r, colour.g, colour.b]
  }

  /** The tile on its page, with the material to draw it with; the tile is upscaled onto a page first if it is new. */
  forTile(rect: TileRect): PlacedTile | null {
    const key = `${rect.atlas}|${rect.sx},${rect.sy},${rect.w},${rect.h},${rect.ox},${rect.oy},${rect.cell}`
    const cached = this.placed.get(key)
    if (cached) return cached
    const atlas = this.atlas(rect.atlas)
    if (!atlas || rect.w <= 0 || rect.h <= 0) return null
    let page = this.pages.at(-1)
    let slot = page?.enhanced.place(rect, atlas) ?? null
    if (!slot) {
      page = this.addPage()
      slot = page.enhanced.place(rect, atlas)
    }
    if (!page || !slot) return null
    this.drawn.push({ page, slot })
    const placed = { ...page.material, rect: { ...rect, atlas: pageName(this.pages.indexOf(page)), sx: slot.sx, sy: slot.sy }, label: slot.label }
    if (slot.label && slot.label.glow > 0) this.glowing = true
    this.placed.set(key, placed)
    return placed
  }

  /**
   * Upload the tiles drawn since the last call, each on its own, and copy them into their pages on the GPU. A
   * page not uploaded yet is uploaded whole by the first copy. The mipmaps of a page are made again once, with
   * its last copy.
   */
  upload(copier: TextureCopier) {
    const drawn = this.drawn.splice(0)
    drawn.forEach(({ page, slot }, i) => {
      const last = !drawn.slice(i + 1).some((d) => d.page === page)
      copyPixels(copier, slot.colour, page.colour, slot, last)
      copyPixels(copier, slot.normal, page.normal, slot, last)
      copyPixels(copier, slot.surface, page.surface, slot, last)
    })
  }

  /** The materials of a page, by the atlas name its placed tiles carry. */
  material(page: string): AtlasMaterial | null {
    return this.pages.find((_, i) => pageName(i) === page)?.material ?? null
  }

  dispose() {
    for (const page of this.pages) {
      page.material.material.dispose()
      page.material.sprite.dispose()
      page.material.actor.dispose()
      page.colour.dispose()
      page.normal.dispose()
      page.surface.dispose()
    }
    this.pages.length = 0
    this.glowing = false
    this.placed.clear()
    this.drawn.length = 0
  }

  /** The atlas's own pixels, to read tiles from. */
  private atlas(name: string): CanvasRenderingContext2D | null {
    const cached = this.atlases.get(name)
    if (cached !== undefined) return cached
    const source = atlasImage(this.tiles, name)
    let ctx: CanvasRenderingContext2D | null = null
    if (source) {
      const canvas = document.createElement('canvas')
      canvas.width = source.width
      canvas.height = source.height
      ctx = canvas.getContext('2d', { willReadFrequently: true })
      ctx?.drawImage(source.image, 0, 0)
    }
    this.atlases.set(name, ctx)
    return ctx
  }

  private addPage(): Page {
    const enhanced = new EnhancedPage(this.paint)
    const colour = pageTexture(enhanced.size)
    colour.colorSpace = THREE.SRGBColorSpace
    colour.anisotropy = 4
    colour.magFilter = this.paint.upscale === 'pixel' ? THREE.NearestFilter : THREE.LinearFilter
    const normal = pageTexture(enhanced.size)
    const surface = pageTexture(enhanced.size)
    const material = new ClayMaterial({ vertexColors: true, alphaTest: 0.5, side: THREE.DoubleSide, map: colour })
    const sprite = material.clone()
    sprite.alphaTest = material.alphaTest
    dressSprite(sprite, { colour, normal, surface }, this.memory)
    dressGround(material, { colour, normal, surface }, this.memory)
    const page = { enhanced, colour, normal, surface, material: { material, sprite, actor: actorMaterial(sprite, { colour, normal, surface }), width: PAGE_TEXELS, height: PAGE_TEXELS } }
    this.pages.push(page)
    return page
  }
}

/** A page on the GPU, made empty: its tiles are copied into it one by one, so the page is never uploaded whole. */
function pageTexture(size: number): THREE.DataTexture {
  const page = new THREE.DataTexture(null, size, size)
  page.needsUpdate = true
  page.source.dataReady = false
  page.generateMipmaps = true
  page.minFilter = THREE.LinearMipmapLinearFilter
  page.magFilter = THREE.LinearFilter
  return page
}

function copyPixels(copier: TextureCopier, pixels: ImageData, page: THREE.Texture, at: { x: number; y: number }, mipmaps: boolean) {
  const tile = new THREE.DataTexture(new Uint8Array(pixels.data.buffer), pixels.width, pixels.height)
  tile.colorSpace = page.colorSpace
  tile.generateMipmaps = false
  tile.needsUpdate = true
  page.generateMipmaps = mipmaps
  copier.copyTextureToTexture(tile, page, null, new THREE.Vector2(at.x, at.y))
  page.generateMipmaps = true
  tile.dispose()
}

function pageName(index: number) {
  return `page:${index}`
}

/** A soft round spot fading from `inner` to transparent, for blob shadows and glows. */
export function radialTexture(inner: string): THREE.Texture {
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = 64
  const ctx = canvas.getContext('2d')
  if (ctx) {
    const gradient = ctx.createRadialGradient(32, 32, 2, 32, 32, 32)
    gradient.addColorStop(0, inner)
    gradient.addColorStop(1, 'rgba(0,0,0,0)')
    ctx.fillStyle = gradient
    ctx.fillRect(0, 0, 64, 64)
  }
  const texture = new THREE.CanvasTexture(canvas)
  texture.colorSpace = THREE.SRGBColorSpace
  return texture
}

/** An additive orange glow for a torch flame. */
export function glowMaterial(): THREE.SpriteMaterial {
  return new THREE.SpriteMaterial({ map: radialTexture('rgba(255,190,110,1)'), color: 0xffb060, blending: THREE.AdditiveBlending, depthWrite: false, transparent: true })
}

function seeded(seed: number) {
  let state = seed >>> 0
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0
    return state / 4294967296
  }
}

function canvasTexture(canvas: HTMLCanvasElement, smooth: boolean): THREE.CanvasTexture {
  const texture = new THREE.CanvasTexture(canvas)
  texture.colorSpace = THREE.SRGBColorSpace
  texture.magFilter = smooth ? THREE.LinearFilter : THREE.NearestFilter
  texture.minFilter = smooth ? THREE.LinearMipmapLinearFilter : THREE.NearestFilter
  return texture
}

/** A door leaf in the DCSS manner: dark planks, two iron bands with rivets and a ring, 32 by 48 texels. */
export function doorTexture(): THREE.CanvasTexture {
  const w = 32
  const h = 48
  const canvas = document.createElement('canvas')
  canvas.width = w
  canvas.height = h
  const ctx = canvas.getContext('2d')
  if (!ctx) return canvasTexture(canvas, true)
  const random = seeded(7)
  const px = (x: number, y: number, colour: string) => {
    ctx.fillStyle = colour
    ctx.fillRect(x, y, 1, 1)
  }
  const planks = [[92, 60, 34], [80, 52, 30], [88, 57, 32], [76, 49, 28]]
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const [r, g, b] = planks[Math.floor(x / 8)]
      const grain = (random() - 0.5) * 14 + (x % 8 === 3 && random() < 0.6 ? -10 : 0)
      px(x, y, x % 8 === 0 ? 'rgb(38,24,14)' : `rgb(${r + grain},${g + grain},${b + grain * 0.6})`)
    }
  for (const band of [7, 38]) {
    for (let x = 0; x < w; x++) {
      px(x, band, 'rgb(112,112,120)')
      px(x, band + 1, 'rgb(72,72,80)')
      px(x, band + 2, 'rgb(46,46,52)')
    }
    for (let x = 4; x < w; x += 8) px(x, band + 1, 'rgb(150,150,158)')
  }
  for (let a = 0; a < 16; a++) {
    const angle = (a / 16) * Math.PI * 2
    px(Math.round(24 + Math.cos(angle) * 2.5), Math.round(25 + Math.sin(angle) * 2.5), 'rgb(130,130,138)')
  }
  ctx.strokeStyle = 'rgb(30,18,10)'
  ctx.strokeRect(0.5, 0.5, w - 1, h - 1)
  return canvasTexture(canvas, true)
}

/** A faint stone grain for wall tops, mid grey so vertex colours set the hue. */
export function capTexture(): THREE.CanvasTexture {
  const size = 32
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = size
  const ctx = canvas.getContext('2d')
  if (!ctx) return canvasTexture(canvas, true)
  const random = seeded(11)
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++) {
      const v = Math.round(205 + (random() - 0.5) * 40)
      ctx.fillStyle = `rgb(${v},${v},${v})`
      ctx.fillRect(x, y, 1, 1)
    }
  ctx.strokeStyle = 'rgb(150,150,150)'
  ctx.strokeRect(0.5, 0.5, size - 1, size - 1)
  return canvasTexture(canvas, false)
}
