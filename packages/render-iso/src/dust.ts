import * as THREE from 'three/webgpu'
import { float, instancedBufferAttribute, sin, cos, time, vec3 } from 'three/tsl'
import { radialTexture } from './materials.js'
import { dustStrength } from './shading.js'

/** Motes around each lit torch, how far they wander from it (cells), how high they rise, and how fast. */
const MOTES_PER_TORCH = 24
const SPREAD = 0.45
const RISE = 0.8
const SPEED = 0.12
const SIZE = 0.09
/** Where the motes gather: this far from the torch toward the middle of the open cell it hangs over. */
const OUT_FROM_WALL = 0.55
const COLOUR = 0xffc890

/**
 * Dust in the light of the lit torches: motes that drift up in the air before each torch and fade in and out, one instanced
 * sprite for all of them. Each mote moves in the shader, so a frame does no work for them; the motes are placed
 * again only when the lit torches change.
 */
export class TorchDust {
  private readonly material = new THREE.SpriteNodeMaterial({ map: radialTexture('rgba(255,220,180,1)'), color: COLOUR, blending: THREE.AdditiveBlending, depthWrite: false, transparent: true })
  readonly mesh = new THREE.Sprite(this.material)
  private readonly seeds: THREE.InstancedBufferAttribute
  private key = ''

  /** `torches`: the most torches lit at once. */
  constructor(torches: number) {
    this.seeds = new THREE.InstancedBufferAttribute(new Float32Array(torches * MOTES_PER_TORCH * 4), 4)
    this.mesh.count = 0
    this.mesh.frustumCulled = false
    this.mesh.renderOrder = 5
    this.place()
  }

  /** Put the motes in the air before these torches, over the open cell each hangs over; a torch that is null is not lit. */
  update(torches: readonly ({ position: THREE.Vector3; cell: { x: number; y: number } } | null)[]) {
    const lit = torches.flatMap((t) => (t ? [new THREE.Vector3(t.cell.x + 0.5, t.position.y, t.cell.y + 0.5).lerp(t.position, 1 - OUT_FROM_WALL)] : []))
    const key = lit.map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)},${p.z.toFixed(2)}`).join(';')
    if (key === this.key) return
    this.key = key
    const count = Math.min(lit.length * MOTES_PER_TORCH, this.seeds.count)
    const random = seeded(lit.length)
    lit.forEach((p, t) => {
      for (let m = 0; m < MOTES_PER_TORCH && t * MOTES_PER_TORCH + m < count; m++) {
        const i = (t * MOTES_PER_TORCH + m) * 4
        const angle = random() * Math.PI * 2
        const reach = Math.sqrt(random()) * SPREAD
        this.seeds.array[i] = p.x + Math.cos(angle) * reach
        this.seeds.array[i + 1] = p.y - RISE / 2
        this.seeds.array[i + 2] = p.z + Math.sin(angle) * reach
        this.seeds.array[i + 3] = random() * 100
      }
    })
    this.seeds.needsUpdate = true
    this.mesh.count = count
  }

  private place() {
    const seed = instancedBufferAttribute<'vec4'>(this.seeds)
    const phase = seed.w
    const age = time.mul(SPEED).add(phase.mul(0.137)).fract()
    const wander = vec3(sin(time.mul(0.7).add(phase)).mul(0.08), age.mul(RISE), cos(time.mul(0.5).add(phase.mul(1.7))).mul(0.08))
    this.material.positionNode = seed.xyz.add(wander)
    this.material.scaleNode = float(SIZE)
    this.material.opacityNode = sin(age.mul(Math.PI)).mul(dustStrength)
  }

  dispose() {
    this.material.map?.dispose()
    this.material.dispose()
  }
}

function seeded(seed: number) {
  let state = (seed * 2654435761) >>> 0
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0
    return state / 4294967296
  }
}
