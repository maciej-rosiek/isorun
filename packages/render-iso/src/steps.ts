import * as THREE from 'three/webgpu'
import { float, instancedDynamicBufferAttribute, length, positionWorld, texture, uniform, uv, vec2 } from 'three/tsl'
import { noiseTexture } from './fog.js'

export interface StepSettings {
  /** The colour of the dust and the footprints. */
  colour: number
  /** How much dust a step raises, 0 for none. */
  dust: number
  /** Seconds a footprint stays on the floor, 0 for no footprints. */
  prints: number
}

interface Puff {
  position: THREE.Vector3
  velocity: THREE.Vector3
  age: number
  life: number
  size: number
}

interface Print {
  x: number
  z: number
  angle: number
  age: number
}

interface Landing {
  delay: number
  from: { x: number; y: number }
  to: { x: number; y: number }
}

const MAX_PUFFS = 192
const MAX_PRINTS = 160
const LIFT_PUFFS = 2
const LAND_PUFFS = 5
const PUFF_HEIGHT = 0.06
const PUFF_RISE = 0.25
const PUFF_SPREAD = 0.7
const PUFF_DRAG = 3
const PUFF_GROWTH = 2.2
const PUFF_OPACITY = 0.45
const PRINT_HEIGHT = 0.011
const PRINT_LENGTH = 0.22
const PRINT_WIDTH = 0.11
const PRINT_APART = 0.09
const PRINT_STAGGER = 0.06
const PRINT_OPACITY = 0.75
const PRINT_FADE = 0.4
const PRINT_DARKNESS = 0.15

const random = (min: number, max: number) => min + Math.random() * (max - min)

/** Dust that puffs up when an actor lifts off and lands, and footprints that fade, so a walk shows. */
export class StepLayer {
  readonly group = new THREE.Group()
  private readonly colour = uniform(new THREE.Color())
  private readonly puffMesh: THREE.InstancedMesh
  private readonly printMesh: THREE.InstancedMesh
  private readonly puffFade = new THREE.InstancedBufferAttribute(new Float32Array(MAX_PUFFS), 1)
  private readonly printFade = new THREE.InstancedBufferAttribute(new Float32Array(MAX_PRINTS), 1)
  private readonly puffs: Puff[] = []
  private readonly prints: Print[] = []
  private readonly landings: Landing[] = []
  private nextPrint = 0
  private settings: StepSettings = { colour: 0x8a7a66, dust: 1, prints: 6 }
  private floor: (x: number, y: number) => boolean = () => false

  constructor() {
    const noise = noiseTexture()
    const puffMaterial = new THREE.MeshLambertNodeMaterial({ transparent: true, depthWrite: false })
    puffMaterial.colorNode = this.colour
    const puffy = texture(noise, uv().mul(0.4).add(positionWorld.xz.mul(0.7))).r
    const round = float(1).sub(length(uv().sub(0.5)).mul(2)).max(0)
    puffMaterial.opacityNode = round.mul(puffy).mul(2).smoothstep(0.1, 0.9).mul(instancedDynamicBufferAttribute(this.puffFade, 'float')).mul(PUFF_OPACITY)
    this.puffMesh = new THREE.InstancedMesh(new THREE.PlaneGeometry(1, 1), puffMaterial, MAX_PUFFS)
    this.puffMesh.renderOrder = 5
    const printMaterial = new THREE.MeshLambertNodeMaterial({ transparent: true, depthWrite: false })
    printMaterial.colorNode = this.colour.mul(PRINT_DARKNESS)
    const oval = length(uv().sub(0.5).mul(vec2(2, 2)))
    printMaterial.opacityNode = oval.smoothstep(1, 0.6).mul(instancedDynamicBufferAttribute(this.printFade, 'float')).mul(PRINT_OPACITY)
    this.printMesh = new THREE.InstancedMesh(new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2), printMaterial, MAX_PRINTS)
    this.printMesh.renderOrder = 2
    for (const mesh of [this.puffMesh, this.printMesh]) {
      mesh.count = 0
      mesh.frustumCulled = false
      this.group.add(mesh)
    }
  }

  configure(settings: StepSettings) {
    this.settings = settings
    this.colour.value.set(settings.colour)
  }

  /** An actor hops from one cell to the next and lands after `seconds`. `floor` says on which cells dust and footprints show. */
  step(from: { x: number; y: number }, to: { x: number; y: number }, seconds: number, floor: (x: number, y: number) => boolean) {
    this.floor = floor
    if (floor(from.x, from.y)) this.puff(from, to, LIFT_PUFFS, 0.6)
    this.landings.push({ delay: seconds, from, to })
  }

  /** Move the dust on and fade the footprints; every puff faces the camera. */
  update(dt: number, camera: THREE.Camera) {
    for (let i = 0; i < this.landings.length; ) {
      const landing = this.landings[i]
      landing.delay -= dt
      if (landing.delay > 0) {
        i++
        continue
      }
      this.landings.splice(i, 1)
      if (!this.floor(landing.to.x, landing.to.y)) continue
      this.puff(landing.to, landing.from, LAND_PUFFS, 1)
      this.footprints(landing.from, landing.to)
    }
    for (const p of this.puffs) {
      p.age += dt
      p.velocity.multiplyScalar(Math.exp(-PUFF_DRAG * dt))
      p.position.addScaledVector(p.velocity, dt)
    }
    let kept = 0
    for (const p of this.puffs) if (p.age < p.life) this.puffs[kept++] = p
    this.puffs.length = kept
    const matrix = new THREE.Matrix4()
    const scale = new THREE.Vector3()
    this.puffs.forEach((p, i) => {
      const t = p.age / p.life
      const size = p.size * (1 + t * PUFF_GROWTH)
      matrix.compose(p.position, camera.quaternion, scale.set(size, size, 1))
      this.puffMesh.setMatrixAt(i, matrix)
      this.puffFade.setX(i, Math.min(1, t * 6) * (1 - t) * (1 - t))
    })
    this.puffMesh.count = this.puffs.length
    this.puffMesh.instanceMatrix.needsUpdate = true
    this.puffFade.needsUpdate = true

    const stay = this.settings.prints
    for (const print of this.prints) print.age += dt
    const rotation = new THREE.Quaternion()
    const axis = new THREE.Vector3(0, 1, 0)
    this.prints.forEach((print, i) => {
      rotation.setFromAxisAngle(axis, print.angle)
      matrix.compose(new THREE.Vector3(print.x, PRINT_HEIGHT, print.z), rotation, scale.set(PRINT_WIDTH, 1, PRINT_LENGTH))
      this.printMesh.setMatrixAt(i, matrix)
      const left = stay > 0 ? 1 - print.age / stay : 0
      this.printFade.setX(i, Math.min(1, Math.max(0, left / PRINT_FADE)))
    })
    this.printMesh.count = this.prints.length
    this.printMesh.instanceMatrix.needsUpdate = true
    this.printFade.needsUpdate = true
  }

  /** Remove the dust and the footprints, for a new level. */
  clear() {
    this.puffs.length = 0
    this.prints.length = 0
    this.landings.length = 0
    this.nextPrint = 0
  }

  dispose() {
    for (const mesh of [this.puffMesh, this.printMesh]) mesh.dispose()
  }

  /** Puffs of dust at the foot of the cell, blown away from `away`. */
  private puff(at: { x: number; y: number }, away: { x: number; y: number }, count: number, strength: number) {
    const dx = at.x - away.x
    const dy = at.y - away.y
    const len = Math.hypot(dx, dy) || 1
    for (let i = 0; i < Math.round(count * this.settings.dust); i++) {
      if (this.puffs.length >= MAX_PUFFS) return
      const spread = random(0, Math.PI * 2)
      const velocity = new THREE.Vector3(Math.cos(spread) * PUFF_SPREAD + (dx / len) * 0.3, random(0.3, 1) * PUFF_RISE, Math.sin(spread) * PUFF_SPREAD + (dy / len) * 0.3).multiplyScalar(strength)
      const position = new THREE.Vector3(at.x + 0.5 + random(-0.12, 0.12), PUFF_HEIGHT, at.y + 0.5 + random(-0.12, 0.12))
      this.puffs.push({ position, velocity, age: 0, life: random(0.5, 0.9), size: random(0.12, 0.2) * strength })
    }
  }

  /** A pair of footprints on the landing cell, along the way of the step. */
  private footprints(from: { x: number; y: number }, to: { x: number; y: number }) {
    if (this.settings.prints <= 0) return
    const dx = to.x - from.x
    const dy = to.y - from.y
    const len = Math.hypot(dx, dy) || 1
    const angle = Math.atan2(dx, dy)
    for (const side of [-1, 1]) {
      const along = side * PRINT_STAGGER
      const x = to.x + 0.5 + (dx / len) * along + (dy / len) * side * PRINT_APART
      const z = to.y + 0.5 + (dy / len) * along - (dx / len) * side * PRINT_APART
      const print = { x, z, angle, age: 0 }
      if (this.prints.length < MAX_PRINTS) this.prints.push(print)
      else this.prints[this.nextPrint] = print
      this.nextPrint = (this.nextPrint + 1) % MAX_PRINTS
    }
  }
}
