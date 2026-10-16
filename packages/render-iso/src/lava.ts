import * as THREE from 'three/webgpu'
import { attribute, cameraWorldMatrix, mix, positionWorld, texture, time, uniform, vec2, vec4 } from 'three/tsl'
import type { Scene } from '@isorun/scene'
import { noiseTexture } from './fog.js'
import { emissionOutput } from './post.js'
import { LiquidGrid, liquidNormal, liquidSight } from './water.js'
import { liquidDepth, type MemoryMask } from './shading.js'

export interface LavaSettings {
  enabled: boolean
  crust: number
  molten: number
  hot: number
  glow: number
  speed: number
}

const DRIFT = 0.02
const PLATE_SCALE = 0.45
const FINE_SCALE = 0.85
const PULSE_SCALE = 0.12
const EDGE_SCALE = 0.9
const EDGE_WOBBLE = 0.1
const CRACK_WIDTH = 0.08
const FINE_CRACK_WIDTH = 0.04
const RIM_OPACITY = 0.45
const BRIGHTNESS = 0.9
/** How far under the crust the molten rock seen through a crack lies, in cells, and how domed the crust's plates are. */
const MOLTEN_DEPTH = 0.12
const PLATE_TILT = 0.5

/** A Lambert material with its own light: the crust takes the scene's light, the molten rock gives its own. */
class LitLavaMaterial extends THREE.MeshLambertNodeMaterial {
  emissiveNode: THREE.Node<'vec3'> | null = null
}

export class LavaLayer {
  readonly mesh: THREE.Mesh
  private readonly grid = new LiquidGrid('lava')
  private readonly crust = uniform(new THREE.Color())
  private readonly molten = uniform(new THREE.Color())
  private readonly hot = uniform(new THREE.Color())
  private readonly glow = uniform(0)
  private readonly speed = uniform(0)

  constructor(memory: MemoryMask) {
    const noise = noiseTexture()
    const place = positionWorld.xz
    const clock = time.mul(this.speed).mul(DRIFT)
    const flow = (scale: number, dx: number, dy: number, at: THREE.Node<'vec2'> = place) => texture(noise, at.mul(scale).add(vec2(clock.mul(dx), clock.mul(dy)))).r
    const lava = attribute<'float'>('liquid', 'float')
    const near = attribute<'float'>('near', 'float')
    const depth = attribute<'float'>('depth', 'float')
    const { shade, live } = liquidSight(memory)
    const edge = lava.add(flow(EDGE_SCALE, 0.2, -0.3).sub(0.5).mul(EDGE_WOBBLE))
    const shape = edge.smoothstep(0.47, 0.53)
    const crack = (value: THREE.Node<'float'>, width: number) => value.sub(0.5).abs().smoothstep(width, 0)
    const cracksAt = (at: THREE.Node<'vec2'>) => crack(flow(PLATE_SCALE, 1, 0.4, at), CRACK_WIDTH).max(crack(flow(FINE_SCALE, -0.6, 0.8, at), FINE_CRACK_WIDTH).mul(0.45))
    const view = cameraWorldMatrix.mul(vec4(0, 0, -1, 0)).xyz
    const deep = place.add(view.xz.div(view.y.negate().max(0.2)).mul(liquidDepth).mul(MOLTEN_DEPTH))
    const pulse = flow(PULSE_SCALE, 0.3, 0.2).mul(0.6).add(0.7)
    const cooling = edge.smoothstep(0.5, 0.8)
    const heatOf = (cracks: THREE.Node<'float'>) => cracks.mul(pulse).add(depth.mul(0.12)).mul(cooling.mul(0.8).add(0.2)).clamp(0, 1)
    const heat = heatOf(cracksAt(place))
    const below = heatOf(cracksAt(deep).max(cracksAt(place)))
    const melt = mix(this.molten, this.hot, below.mul(below))
    const open = heat.smoothstep(0.15, 0.6)
    const material = new LitLavaMaterial({ transparent: true, depthWrite: false })
    const rim = near.smoothstep(0, 0.5).pow(2).mul(shape.oneMinus()).mul(live)
    const crust = this.crust.mul(flow(PULSE_SCALE * 4, -0.2, 0.1).mul(0.7).add(0.65))
    material.colorNode = crust.mul(open.oneMinus()).mul(shade)
    material.normalNode = liquidNormal((at) => cracksAt(at).oneMinus(), place, PLATE_TILT)
    material.emissiveNode = melt.mul(BRIGHTNESS).mul(open).add(this.molten.mul(rim)).mul(shade)
    material.opacityNode = shape.max(rim.mul(RIM_OPACITY))
    material.mrtNode = emissionOutput(melt.mul(below).mul(shape).mul(live).mul(this.glow))
    this.mesh = new THREE.Mesh(this.grid.geometry, material)
    this.mesh.renderOrder = 4
  }

  get shown(): boolean {
    return this.mesh.visible && this.grid.shown
  }

  configure(settings: LavaSettings) {
    this.crust.value.set(settings.crust)
    this.molten.value.set(settings.molten)
    this.hot.value.set(settings.hot)
    this.glow.value = settings.glow
    this.speed.value = settings.speed
    this.mesh.visible = settings.enabled
  }

  build(scene: Scene, revision: number) {
    if (this.grid.build(scene, revision)) this.mesh.geometry = this.grid.geometry
  }

  dispose() {
    this.grid.dispose()
  }
}
