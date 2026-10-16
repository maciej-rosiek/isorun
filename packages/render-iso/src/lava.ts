import * as THREE from 'three/webgpu'
import { attribute, mix, positionWorld, texture, time, uniform, vec2 } from 'three/tsl'
import type { Scene } from '@isorun/scene'
import { noiseTexture } from './fog.js'
import { emissionOutput } from './post.js'
import { LiquidGrid } from './water.js'

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

export class LavaLayer {
  readonly mesh: THREE.Mesh
  private readonly grid = new LiquidGrid('lava')
  private readonly crust = uniform(new THREE.Color())
  private readonly molten = uniform(new THREE.Color())
  private readonly hot = uniform(new THREE.Color())
  private readonly glow = uniform(0)
  private readonly speed = uniform(0)

  constructor() {
    const noise = noiseTexture()
    const place = positionWorld.xz
    const clock = time.mul(this.speed).mul(DRIFT)
    const flow = (scale: number, dx: number, dy: number) => texture(noise, place.mul(scale).add(vec2(clock.mul(dx), clock.mul(dy)))).r
    const lava = attribute<'float'>('liquid', 'float')
    const near = attribute<'float'>('near', 'float')
    const depth = attribute<'float'>('depth', 'float')
    const live = attribute<'float'>('live', 'float')
    const shade = attribute<'float'>('shade', 'float')
    const edge = lava.add(flow(EDGE_SCALE, 0.2, -0.3).sub(0.5).mul(EDGE_WOBBLE))
    const shape = edge.smoothstep(0.47, 0.53)
    const crack = (value: THREE.Node<'float'>, width: number) => value.sub(0.5).abs().smoothstep(width, 0)
    const cracks = crack(flow(PLATE_SCALE, 1, 0.4), CRACK_WIDTH).max(crack(flow(FINE_SCALE, -0.6, 0.8), FINE_CRACK_WIDTH).mul(0.45))
    const pulse = flow(PULSE_SCALE, 0.3, 0.2).mul(0.6).add(0.7)
    const cooling = edge.smoothstep(0.5, 0.8)
    const heat = cracks.mul(pulse).add(depth.mul(0.12)).mul(cooling.mul(0.8).add(0.2)).clamp(0, 1)
    const melt = mix(this.molten, this.hot, heat.mul(heat))
    const material = new THREE.MeshBasicNodeMaterial({ transparent: true, depthWrite: false })
    const rim = near.smoothstep(0, 0.5).pow(2).mul(shape.oneMinus()).mul(live)
    const crust = this.crust.mul(flow(PULSE_SCALE * 4, -0.2, 0.1).mul(0.7).add(0.65))
    material.colorNode = mix(mix(crust, melt.mul(BRIGHTNESS), heat.smoothstep(0.15, 0.6)), this.molten, rim).mul(shade)
    material.opacityNode = shape.max(rim.mul(RIM_OPACITY))
    material.mrtNode = emissionOutput(melt.mul(heat).mul(shape).mul(live).mul(this.glow))
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

  build(scene: Scene) {
    if (this.grid.build(scene)) this.mesh.geometry = this.grid.geometry
  }

  dispose() {
    this.grid.dispose()
  }
}
