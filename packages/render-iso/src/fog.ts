import * as THREE from 'three/webgpu'
import { attribute, mix, positionWorld, texture, time, uniform, vec2 } from 'three/tsl'
import type { Scene, SceneCell } from '@isorun/scene'

export interface FogSettings {
  enabled: boolean
  colour: number
  /** How thick the mist is, 0 to 1. */
  density: number
  /** Cells above the floor of the top layer. */
  height: number
  /** Cells the fog reaches from the explored floor into the unexplored cells; known walls stop it. */
  reach: number
  /** How gradual the puffy edge of the fog is, 0 (a sharp edge) to 1. */
  softness: number
}

const NOISE_SIZE = 128
const NOISE_OCTAVES = [8, 16, 32]
const NOISE_SCALE = 0.12
const DRIFT = 0.015
const LAYERS = 10
const LAYER_OPACITY = 0.3
const SHAPE_BLUR = 0.6
const SHAPE_GAIN = 2
const EDGE = 0.35
const EDGE_NOISE = 0.8
const DOME = 0.45
const VISIBLE_FOG = 0.002
const DIAGONAL = Math.SQRT2
const CHAMFER_ROUNDS = 3
const FADE_SECONDS = 0.8

/** A drifting fog over the unexplored cells the explored floor opens onto: stacked layers with a dense body and a puffy edge that the noise moves, lower at the explored edge and at the reach. It never covers an explored cell on the screen. A change of the explored area blends the fog from where it is to the new shape. */
export class FogLayer {
  readonly mesh: THREE.Mesh
  private enabled = true
  private readonly colour = uniform(new THREE.Color())
  private readonly density = uniform(0)
  private readonly softness = uniform(0)
  private readonly parallel = uniform(new THREE.Vector3(0, -1, 0))
  private readonly eye = uniform(new THREE.Vector3())
  private readonly perspective = uniform(0)
  private readonly blend = uniform(1)
  private readonly origin = uniform(new THREE.Vector2())
  private readonly size = uniform(new THREE.Vector2(1, 1))
  private readonly fieldFrom = texture(cellTexture(new Uint8Array(1), 1, 1))
  private readonly fieldTo = texture(cellTexture(new Uint8Array(1), 1, 1))
  private readonly maskFrom = texture(cellTexture(new Uint8Array(1), 1, 1))
  private readonly maskTo = texture(cellTexture(new Uint8Array(1), 1, 1))
  private grid = { x0: 0, y0: 0, w: 0, h: 0 }
  private from = { field: new Uint8Array(0), mask: new Uint8Array(0) }
  private to = { field: new Uint8Array(0), mask: new Uint8Array(0) }
  private shapeKey = ''
  private height = 0

  constructor() {
    const noise = noiseTexture()
    const layer = attribute<'float'>('layer', 'float')
    const place = positionWorld.xz.add(layer.mul(3.7))
    const slow = texture(noise, place.mul(NOISE_SCALE).add(vec2(time.mul(DRIFT), time.mul(DRIFT * 0.6)))).r
    const fast = texture(noise, place.mul(NOISE_SCALE * 2.3).sub(vec2(time.mul(DRIFT * 1.7), time.mul(DRIFT * 0.4)))).r
    const puffs = slow.mul(0.6).add(fast.mul(0.4)).smoothstep(0.25, 0.75)
    const blend = this.blend.smoothstep(0, 1)
    const here = positionWorld.xz.sub(this.origin).div(this.size)
    const fog = mix(this.fieldFrom.sample(here).r, this.fieldTo.sample(here).r, blend)
    const ray = mix(this.parallel, positionWorld.sub(this.eye), this.perspective)
    const ground = positionWorld.xz.sub(ray.xz.mul(positionWorld.y.div(ray.y.min(-0.01)))).sub(this.origin).div(this.size)
    const explored = mix(this.maskFrom.sample(ground).r, this.maskTo.sample(ground).r, blend).smoothstep(0.2, 0.6)
    const body = fog.add(puffs.sub(0.5).mul(EDGE_NOISE)).sub(layer.mul(DOME))
    const width = this.softness.mul(EDGE).add(0.02)
    const material = new THREE.MeshBasicNodeMaterial({ transparent: true, depthWrite: false, side: THREE.DoubleSide })
    material.colorNode = this.colour
    material.opacityNode = body.smoothstep(width.negate().add(EDGE), width.add(EDGE)).mul(fog.smoothstep(0, 0.1)).mul(explored.oneMinus()).mul(this.density).mul(LAYER_OPACITY)
    this.mesh = new THREE.Mesh(fogGeometry([], [], []), material)
    this.mesh.renderOrder = 5
    this.mesh.visible = false
  }

  configure(settings: FogSettings) {
    this.colour.value.set(settings.colour)
    this.density.value = settings.density
    this.softness.value = settings.softness
    this.enabled = settings.enabled
    this.mesh.visible = this.enabled && (this.mesh.geometry.index?.count ?? 0) > 0
  }

  /** Where the camera is and looks: the fog finds the ground cell behind each of its points on the screen. */
  view(camera: THREE.OrthographicCamera | THREE.PerspectiveCamera) {
    camera.getWorldDirection(this.parallel.value)
    this.eye.value.copy(camera.position)
    this.perspective.value = camera instanceof THREE.PerspectiveCamera ? 1 : 0
  }

  /** Moves the blend to the new fog on by `dt` seconds. */
  update(dt: number) {
    this.blend.value = Math.min(1, this.blend.value + dt / FADE_SECONDS)
  }

  /** The fog over the unexplored cells the explored floor opens onto, in layers from the floor up to `height`; the same explored area keeps the fog as it is. */
  build(scene: Scene, height: number, reach: number) {
    const explored = [...scene.cells.values()].filter((c) => c.visibility !== 'unseen')
    const shapeKey = `${reach}|${explored.map((c) => `${c.x},${c.y}${c.occluder ? '#' : ''}`).join(' ')}`
    if (shapeKey === this.shapeKey && height === this.height) return
    const all = [...scene.cells.values()]
    if (!all.length) return
    const margin = Math.ceil(reach + 2 * SHAPE_BLUR) + 1
    const x0 = Math.min(...all.map((c) => c.x)) - margin
    const y0 = Math.min(...all.map((c) => c.y)) - margin
    const w = Math.max(...all.map((c) => c.x)) + margin - x0 + 1
    const h = Math.max(...all.map((c) => c.y)) + margin - y0 + 1
    const next = fogShape(explored, reach, x0, y0, w, h)
    if (shapeKey !== this.shapeKey) {
      const t = THREE.MathUtils.smoothstep(this.blend.value, 0, 1)
      const old = this.grid
      const shown = (a: Uint8Array, b: Uint8Array, x: number, y: number, fallback: number) => {
        const ox = x + x0 - old.x0
        const oy = y + y0 - old.y0
        if (ox < 0 || oy < 0 || ox >= old.w || oy >= old.h || a.length !== old.w * old.h) return fallback
        const i = oy * old.w + ox
        return Math.round(THREE.MathUtils.lerp(a[i], b[i], t))
      }
      const field = new Uint8Array(w * h)
      const mask = new Uint8Array(w * h)
      for (let y = 0; y < h; y++)
        for (let x = 0; x < w; x++) {
          const i = y * w + x
          field[i] = shown(this.from.field, this.to.field, x, y, next.field[i])
          mask[i] = shown(this.from.mask, this.to.mask, x, y, next.mask[i])
        }
      this.from = { field, mask }
      this.to = next
      this.blend.value = this.shapeKey === '' ? 1 : 0
    }
    this.shapeKey = shapeKey
    this.height = height
    this.grid = { x0, y0, w, h }
    this.origin.value.set(x0, y0)
    this.size.value.set(w, h)
    for (const [node, data] of [[this.fieldFrom, this.from.field], [this.fieldTo, this.to.field], [this.maskFrom, this.from.mask], [this.maskTo, this.to.mask]] as const) {
      node.value.dispose()
      node.value = cellTexture(data, w, h)
    }
    const positions: number[] = []
    const layers: number[] = []
    const indices: number[] = []
    const foggy = (x: number, y: number) => x >= 0 && y >= 0 && x < w && y < h && (this.from.field[y * w + x] > 0 || this.to.field[y * w + x] > 0)
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        if (![-1, 0, 1].some((dy) => [-1, 0, 1].some((dx) => foggy(x + dx, y + dy)))) continue
        for (let i = 0; i < LAYERS; i++) {
          const base = positions.length / 3
          const layer = i / (LAYERS - 1)
          for (const [cx, cy] of [[x, y], [x + 1, y], [x + 1, y + 1], [x, y + 1]]) {
            positions.push(cx + x0, layer * height, cy + y0)
            layers.push(layer)
          }
          indices.push(base, base + 2, base + 1, base, base + 3, base + 2)
        }
      }
    this.mesh.geometry.dispose()
    this.mesh.geometry = fogGeometry(positions, layers, indices)
    this.mesh.visible = this.enabled && indices.length > 0
  }

  dispose() {
    this.mesh.geometry.dispose()
    for (const node of [this.fieldFrom, this.fieldTo, this.maskFrom, this.maskTo]) node.value.dispose()
  }
}

/** How thick the fog is over each cell of a grid (0 to 255), and which cells are explored (255). */
function fogShape(explored: readonly SceneCell[], reach: number, x0: number, y0: number, w: number, h: number) {
  const distance = new Float32Array(w * h).fill(Infinity)
  const blocked = new Uint8Array(w * h)
  for (const c of explored) {
    const i = (c.y - y0) * w + (c.x - x0)
    if (c.occluder) blocked[i] = 1
    else distance[i] = 0
  }
  const mask = Uint8Array.from({ length: w * h }, (_, i) => (distance[i] === 0 || blocked[i] ? 255 : 0))
  for (let round = 0; round < CHAMFER_ROUNDS; round++) chamfer(distance, blocked, w, h)
  const full = reach / 2
  const thickness = (i: number) => {
    const d = distance[i]
    if (reach <= 0 || blocked[i] || d === 0 || !Number.isFinite(d)) return 0
    return THREE.MathUtils.smoothstep(d, 0, full) * (1 - THREE.MathUtils.smoothstep(d, reach - full, reach))
  }
  const blurred = blur(Float32Array.from({ length: w * h }, (_, i) => thickness(i)), w, h, SHAPE_BLUR)
  const field = Uint8Array.from(blurred, (v) => (v < VISIBLE_FOG ? 0 : Math.round(Math.min(1, v * SHAPE_GAIN) * 255)))
  return { field, mask }
}

/** One texel per cell, 0 to 255, blended between cells. */
function cellTexture(cells: Uint8Array, w: number, h: number): THREE.DataTexture {
  const map = new THREE.DataTexture(cells, w, h, THREE.RedFormat)
  map.magFilter = map.minFilter = THREE.LinearFilter
  map.needsUpdate = true
  return map
}

/** Distance in cells to the nearest zero around blocked cells, in place (a chamfer pass each way, diagonals at the square root of two). */
export function chamfer(d: Float32Array, blocked: Uint8Array, w: number, h: number) {
  const relax = (i: number, j: number, step: number) => {
    if (!blocked[i] && !blocked[j]) d[i] = Math.min(d[i], d[j] + step)
  }
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const i = y * w + x
      if (x > 0) relax(i, i - 1, 1)
      if (y > 0) relax(i, i - w, 1)
      if (x > 0 && y > 0) relax(i, i - w - 1, DIAGONAL)
      if (x < w - 1 && y > 0) relax(i, i - w + 1, DIAGONAL)
    }
  for (let y = h - 1; y >= 0; y--)
    for (let x = w - 1; x >= 0; x--) {
      const i = y * w + x
      if (x < w - 1) relax(i, i + 1, 1)
      if (y < h - 1) relax(i, i + w, 1)
      if (x < w - 1 && y < h - 1) relax(i, i + w + 1, DIAGONAL)
      if (x > 0 && y < h - 1) relax(i, i + w - 1, DIAGONAL)
    }
}

/** A Gaussian blur of a grid of values, `sigma` cells wide; 0 leaves it as it is. */
export function blur(values: Float32Array, w: number, h: number, sigma: number): Float32Array {
  if (sigma <= 0) return values
  const radius = Math.ceil(sigma * 2)
  const kernel = Array.from({ length: radius * 2 + 1 }, (_, i) => Math.exp(-((i - radius) ** 2) / (2 * sigma * sigma)))
  const total = kernel.reduce((a, b) => a + b, 0)
  const pass = (src: Float32Array, dx: number, dy: number) => {
    const out = new Float32Array(src.length)
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        let sum = 0
        for (let k = -radius; k <= radius; k++) {
          const sx = x + k * dx
          const sy = y + k * dy
          if (sx >= 0 && sy >= 0 && sx < w && sy < h) sum += src[sy * w + sx] * kernel[k + radius]
        }
        out[y * w + x] = sum / total
      }
    return out
  }
  return pass(pass(values, 1, 0), 0, 1)
}

/** Tileable value noise, several octaves, 0..255 in every channel. */
export function noiseTexture(): THREE.DataTexture {
  const data = new Uint8Array(NOISE_SIZE * NOISE_SIZE * 4)
  const field = new Float32Array(NOISE_SIZE * NOISE_SIZE)
  let seed = 7
  const random = () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0
    return seed / 4294967296
  }
  let total = 0
  NOISE_OCTAVES.forEach((cells, octave) => {
    const weight = 1 / (octave + 1)
    total += weight
    const grid = Array.from({ length: cells * cells }, random)
    const at = (gx: number, gy: number) => grid[(gy % cells) * cells + (gx % cells)]
    for (let y = 0; y < NOISE_SIZE; y++)
      for (let x = 0; x < NOISE_SIZE; x++) {
        const fx = (x / NOISE_SIZE) * cells
        const fy = (y / NOISE_SIZE) * cells
        const gx = Math.floor(fx)
        const gy = Math.floor(fy)
        const tx = THREE.MathUtils.smoothstep(fx - gx, 0, 1)
        const ty = THREE.MathUtils.smoothstep(fy - gy, 0, 1)
        const top = THREE.MathUtils.lerp(at(gx, gy), at(gx + 1, gy), tx)
        const bottom = THREE.MathUtils.lerp(at(gx, gy + 1), at(gx + 1, gy + 1), tx)
        field[y * NOISE_SIZE + x] += THREE.MathUtils.lerp(top, bottom, ty) * weight
      }
  })
  for (let i = 0; i < field.length; i++) {
    const v = Math.round((field[i] / total) * 255)
    data.set([v, v, v, 255], i * 4)
  }
  const noise = new THREE.DataTexture(data, NOISE_SIZE, NOISE_SIZE)
  noise.wrapS = noise.wrapT = THREE.RepeatWrapping
  noise.magFilter = noise.minFilter = THREE.LinearFilter
  noise.needsUpdate = true
  return noise
}

function fogGeometry(positions: number[], layers: number[], indices: number[]) {
  const geometry = new THREE.BufferGeometry()
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3))
  geometry.setAttribute('layer', new THREE.Float32BufferAttribute(layers, 1))
  geometry.setIndex(indices)
  return geometry
}
