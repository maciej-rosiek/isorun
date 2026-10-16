import * as THREE from 'three/webgpu'
import { attribute, cameraWorldMatrix, float, mix, positionWorld, texture, time, vec2, vec3, vec4 } from 'three/tsl'
import type { Scene } from '@isorun/scene'
import { noiseTexture } from './fog.js'
import { emissionOutput } from './post.js'
import { LiquidGrid, liquidSight } from './water.js'
import type { MemoryMask } from './shading.js'

const DEEP = vec3(0.012, 0.016, 0.05)
const NEBULA = vec3(0.065, 0.026, 0.125)
const STAR = vec3(0.75, 0.85, 1)
const DRIFT = 0.015
const NEBULA_SCALE = 0.18
const STAR_SCALE = 2.6
const SPARKLE_SCALE = 1.7
const TWINKLE_SCALE = 0.9
const EDGE_SCALE = 0.9
const EDGE_WOBBLE = 0.1
const STAR_BRIGHTNESS = 2.5
const STAR_GLOW = 1.5
const OPACITY = 0.95
/** The layers of stars under the surface: how deep each is (cells), how dense and how bright; the nebula lies under them all. */
const LAYERS = [
  { depth: 0.3, scale: 1, brightness: 1 },
  { depth: 1.1, scale: 1.7, brightness: 0.6 },
  { depth: 2.4, scale: 2.6, brightness: 0.35 },
]
const NEBULA_DEPTH = 3.2

export class StarwaterLayer {
  readonly mesh: THREE.Mesh
  private readonly grid = new LiquidGrid('starwater')

  constructor(memory: MemoryMask) {
    const noise = noiseTexture()
    const place = positionWorld.xz
    const clock = time.mul(DRIFT)
    const at = (scale: number, dx: number, dy: number) => texture(noise, place.mul(scale).add(vec2(clock.mul(dx), clock.mul(dy)))).r
    const water = attribute<'float'>('liquid', 'float')
    const { shade, live } = liquidSight(memory)
    const edge = water.add(at(EDGE_SCALE, 0.3, -0.2).sub(0.5).mul(EDGE_WOBBLE))
    const shape = edge.smoothstep(0.47, 0.53)
    const view = cameraWorldMatrix.mul(vec4(0, 0, -1, 0)).xyz
    const below = (depth: number) => place.add(view.xz.div(view.y.negate().max(0.2)).mul(depth))
    const sky = mix(DEEP, NEBULA, texture(noise, below(NEBULA_DEPTH).mul(NEBULA_SCALE).add(vec2(clock, clock.mul(-0.6)))).r.smoothstep(0.4, 0.75))
    const twinkle = at(TWINKLE_SCALE, 9, 6).smoothstep(0.3, 0.75)
    let stars: THREE.Node<'float'> = float(0)
    for (const layer of LAYERS) {
      const p = below(layer.depth)
      const points = texture(noise, p.mul(STAR_SCALE * layer.scale)).r.mul(texture(noise, p.mul(SPARKLE_SCALE * layer.scale).add(0.37)).r).smoothstep(0.42, 0.5)
      stars = stars.add(points.mul(layer.brightness))
    }
    const star = stars.min(1).mul(twinkle).mul(live).mul(edge.smoothstep(0.55, 0.75))
    const material = new THREE.MeshBasicNodeMaterial({ transparent: true, depthWrite: false })
    material.colorNode = mix(sky, STAR.mul(STAR_BRIGHTNESS), star).mul(shade)
    material.opacityNode = shape.mul(OPACITY)
    material.mrtNode = emissionOutput(STAR.mul(star).mul(shape).mul(STAR_GLOW))
    this.mesh = new THREE.Mesh(this.grid.geometry, material)
    this.mesh.renderOrder = 4
  }

  get shown(): boolean {
    return this.mesh.visible && this.grid.shown
  }

  configure(enabled: boolean) {
    this.mesh.visible = enabled
  }

  build(scene: Scene, revision: number) {
    if (this.grid.build(scene, revision)) this.mesh.geometry = this.grid.geometry
  }

  dispose() {
    this.grid.dispose()
  }
}
