import * as THREE from 'three/webgpu'
import { attribute, mix, positionWorld, texture, time, uniform, vec2 } from 'three/tsl'
import { MEMORY_SHADE, getCell, type LiquidKind, type Scene } from '@isorun/scene'
import { blur, chamfer, noiseTexture } from './fog.js'

export interface WaterSettings {
  enabled: boolean
  /** The colour of the water where it is deep, far from the shore. */
  deep: number
  /** The colour of the water near the shore. */
  shallow: number
  /** The colour of the light lines and the foam. */
  light: number
  /** How bright the light lines and the foam are, 0 to 1. */
  strength: number
  /** How fast the water moves, 0 for still water. */
  speed: number
}

const SURFACE_HEIGHT = 0.02
const SUBDIVISIONS = 4
const SHORE_ROUNDING = 0.45
const NEAR_ROUNDING = 0.9
const DEEP_DISTANCE = 1
const SHALLOW_DEPTH = 0.25
const CAUSTIC_SCALE = 0.35
const SWELL_SCALE = 0.15
const FOAM_SCALE = 0.6
const EDGE_SCALE = 0.9
const DRIFT = 0.04
const CAUSTIC_SHARPNESS = 6
const SHALLOW_OPACITY = 0.6
const DEEP_OPACITY = 0.95
const EDGE_WOBBLE = 0.12
const FOAM_WIDTH = 0.08
const AROUND = [-1, 0, 1].flatMap((dy) => [-1, 0, 1].map((dx) => [dx, dy]))

const PALETTES: Record<Exclude<LiquidKind, 'water' | 'lava'>, { deep: number; shallow: number; light: number; strength: number; speed: number }> = {
  murky: { deep: 0x18200f, shallow: 0x0c1207, light: 0x6f7c3c, strength: 1, speed: 0.6 },
  bog: { deep: 0x1d360c, shallow: 0x284612, light: 0x8cc63c, strength: 2, speed: 0.35 },
}

/**
 * A fine grid over the seen cells of one liquid and the land around them; its points carry how much of the liquid is there
 * (rounded at the corners), how deep (shallow cells stay shallow), and whether the cell is in sight.
 */
export class LiquidGrid {
  geometry = new THREE.BufferGeometry()
  private builtKey = ''

  constructor(private readonly kind: LiquidKind) {}

  get shown(): boolean {
    return this.geometry.index !== null && this.geometry.index.count > 0
  }

  build(scene: Scene): boolean {
    const holds = (x: number, y: number) => getCell(scene, x, y)?.liquid?.kind === this.kind
    const pools = [...scene.cells.values()].filter((c) => c.liquid?.kind === this.kind && c.visibility !== 'unseen')
    const key = JSON.stringify(pools.map((c) => [c.x, c.y, c.visibility, c.liquid?.deep, AROUND.map(([dx, dy]) => getCell(scene, c.x + dx, c.y + dy)).map((n) => [n?.visibility, n?.occluder, n?.liquid?.kind])]))
    if (key === this.builtKey) return false
    this.builtKey = key
    this.geometry.dispose()
    this.geometry = new THREE.BufferGeometry()
    const positions: number[] = []
    const amount: number[] = []
    const near: number[] = []
    const depth: number[] = []
    const live: number[] = []
    const shade: number[] = []
    const indices: number[] = []
    const commit = () => {
      this.geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3))
      this.geometry.setAttribute('liquid', new THREE.Float32BufferAttribute(amount, 1))
      this.geometry.setAttribute('near', new THREE.Float32BufferAttribute(near, 1))
      this.geometry.setAttribute('depth', new THREE.Float32BufferAttribute(depth, 1))
      this.geometry.setAttribute('live', new THREE.Float32BufferAttribute(live, 1))
      this.geometry.setAttribute('shade', new THREE.Float32BufferAttribute(shade, 1))
      this.geometry.setIndex(indices)
      return true
    }
    if (!pools.length) return commit()
    const x0 = Math.min(...pools.map((c) => c.x)) - 2
    const y0 = Math.min(...pools.map((c) => c.y)) - 2
    const w = Math.max(...pools.map((c) => c.x)) + 2 - x0 + 1
    const h = Math.max(...pools.map((c) => c.y)) + 2 - y0 + 1
    const cellAt = (i: number) => getCell(scene, x0 + (i % w), y0 + Math.floor(i / w))
    const mask = Float32Array.from({ length: w * h }, (_, i) => (holds(x0 + (i % w), y0 + Math.floor(i / w)) ? 1 : 0))
    const field = blur(mask, w, h, SHORE_ROUNDING)
    const nearField = blur(mask, w, h, NEAR_ROUNDING)
    const distance = mask.map((m) => (m ? Infinity : 0))
    chamfer(distance, new Uint8Array(w * h), w, h)
    chamfer(distance, new Uint8Array(w * h), w, h)
    const deepest = Float32Array.from({ length: w * h }, (_, i) => (cellAt(i)?.liquid?.deep === false ? SHALLOW_DEPTH : 1))
    const depthField = distance.map((d, i) => Math.min(deepest[i], Math.max(0, (d - 0.5) / DEEP_DISTANCE)))
    const sample = (grid: Float32Array, px: number, py: number) => {
      const fx = px - x0 - 0.5
      const fy = py - y0 - 0.5
      const ix = Math.floor(fx)
      const iy = Math.floor(fy)
      const at = (x: number, y: number) => grid[Math.min(h - 1, Math.max(0, y)) * w + Math.min(w - 1, Math.max(0, x))]
      const top = THREE.MathUtils.lerp(at(ix, iy), at(ix + 1, iy), fx - ix)
      const bottom = THREE.MathUtils.lerp(at(ix, iy + 1), at(ix + 1, iy + 1), fx - ix)
      return THREE.MathUtils.lerp(top, bottom, fy - iy)
    }
    const covered = new Map<string, { x: number; y: number }>()
    for (const c of pools)
      for (let dy = -1; dy <= 1; dy++)
        for (let dx = -1; dx <= 1; dx++) {
          const cell = getCell(scene, c.x + dx, c.y + dy)
          if (cell && !cell.occluder && cell.visibility !== 'unseen') covered.set(`${cell.x},${cell.y}`, { x: cell.x, y: cell.y })
        }
    const row = SUBDIVISIONS + 1
    for (const { x, y } of covered.values()) {
      const seen = getCell(scene, x, y)?.visibility === 'visible'
      const base = positions.length / 3
      for (let j = 0; j <= SUBDIVISIONS; j++)
        for (let i = 0; i <= SUBDIVISIONS; i++) {
          const px = x + i / SUBDIVISIONS
          const py = y + j / SUBDIVISIONS
          positions.push(px, SURFACE_HEIGHT, py)
          amount.push(sample(field, px, py))
          near.push(sample(nearField, px, py))
          depth.push(sample(depthField, px, py))
          live.push(seen ? 1 : 0)
          shade.push(seen ? 1 : MEMORY_SHADE)
        }
      for (let j = 0; j < SUBDIVISIONS; j++)
        for (let i = 0; i < SUBDIVISIONS; i++) {
          const a = base + j * row + i
          indices.push(a, a + row + 1, a + 1, a, a + row, a + row + 1)
        }
    }
    return commit()
  }

  dispose() {
    this.geometry.dispose()
  }
}

/** A water surface over the seen water cells: a rounded shore, shallow to deep colour, light lines that drift and foam along the shore. */
export class WaterLayer {
  readonly mesh: THREE.Mesh
  private readonly grid: LiquidGrid
  private readonly deep = uniform(new THREE.Color())
  private readonly shallow = uniform(new THREE.Color())
  private readonly light = uniform(new THREE.Color())
  private readonly strength = uniform(0)
  private readonly speed = uniform(0)

  constructor(private readonly kind: Exclude<LiquidKind, 'lava'>) {
    this.grid = new LiquidGrid(kind)
    const noise = noiseTexture()
    const place = positionWorld.xz
    const clock = time.mul(this.speed).mul(DRIFT)
    const drift = (scale: number, dx: number, dy: number) => texture(noise, place.mul(scale).add(vec2(clock.mul(dx), clock.mul(dy)))).r
    const ridge = (scale: number, dx: number, dy: number) => drift(scale, dx, dy).mul(2).sub(1).abs().oneMinus()
    const water = attribute<'float'>('liquid', 'float')
    const depth = attribute<'float'>('depth', 'float')
    const live = attribute<'float'>('live', 'float')
    const shade = attribute<'float'>('shade', 'float')
    const edge = water.add(drift(EDGE_SCALE, 0.3, -0.2).sub(0.5).mul(EDGE_WOBBLE))
    const shape = edge.smoothstep(0.47, 0.53)
    const caustics = ridge(CAUSTIC_SCALE, 1, 0.6).mul(ridge(CAUSTIC_SCALE * 1.3, -0.7, 0.9)).pow(CAUSTIC_SHARPNESS).mul(depth.mul(-0.5).add(1))
    const swell = drift(SWELL_SCALE, 0.4, -0.3).mul(0.25).add(0.875)
    const foam = edge.smoothstep(0.5 + FOAM_WIDTH, 0.5).mul(drift(FOAM_SCALE, -0.5, -0.8).smoothstep(0.3, 0.6))
    const glow = caustics.add(foam).min(1).mul(this.strength).mul(live)
    const material = new THREE.MeshLambertNodeMaterial({ transparent: true, depthWrite: false })
    material.colorNode = mix(mix(this.shallow, this.deep, depth).mul(swell), this.light, glow).mul(shade)
    material.opacityNode = shape.mul(mix(SHALLOW_OPACITY, DEEP_OPACITY, depth)).max(glow.mul(shape))
    this.mesh = new THREE.Mesh(this.grid.geometry, material)
    this.mesh.renderOrder = 4
  }

  configure(settings: WaterSettings) {
    const palette = this.kind === 'water' ? { ...settings, strength: 1, speed: 1 } : PALETTES[this.kind]
    this.deep.value.set(palette.deep)
    this.shallow.value.set(palette.shallow)
    this.light.value.set(palette.light)
    this.strength.value = Math.min(1, settings.strength * palette.strength)
    this.speed.value = settings.speed * palette.speed
    this.mesh.visible = settings.enabled
  }

  build(scene: Scene) {
    if (this.grid.build(scene)) this.mesh.geometry = this.grid.geometry
  }

  dispose() {
    this.grid.dispose()
  }
}
