import * as THREE from 'three/webgpu'
import { length, positionWorld, texture, uniform, uv } from 'three/tsl'
import { noiseTexture } from './fog.js'

export interface CombatSettings {
  /** The colour of the blood. */
  blood: number
  /** How much blood a hit spills, 0 for none. */
  gore: number
  /** How much the camera shakes at a hit, 0 for a still camera. */
  shake: number
  /** Seconds the actors stop at the moment of a hit; a kill stops them longer. */
  hitStop: number
  /** How bright the flash of light and the sparks at a hit are, 0 for none. */
  flash: number
}

/** A hit on a monster, a kill, or a hit on the player. */
export type ImpactKind = 'hit' | 'death' | 'hurt'

interface Particle {
  position: THREE.Vector3
  velocity: THREE.Vector3
  age: number
  life: number
  size: number
}

interface Splat {
  x: number
  z: number
  size: number
  angle: number
  age: number
  grow: number
}

interface ImpactSpec {
  drops: number
  dropSpeed: number
  sparks: number
  trauma: number
  punch: number
  light: number
  pool: number
  stop: number
}

const IMPACTS: Record<ImpactKind, ImpactSpec> = {
  hit: { drops: 10, dropSpeed: 2.2, sparks: 8, trauma: 0.4, punch: 0.3, light: 1, pool: 0, stop: 1 },
  hurt: { drops: 8, dropSpeed: 2, sparks: 6, trauma: 0.6, punch: 0.5, light: 1, pool: 0, stop: 1 },
  death: { drops: 30, dropSpeed: 3, sparks: 16, trauma: 0.8, punch: 1, light: 1.8, pool: 0.9, stop: 1.8 },
}

const MAX_DROPS = 256
const MAX_SPARKS = 128
const MAX_SPLATS = 160
const GRAVITY = 9.8
const SPARK_GRAVITY = 3
const IMPACT_HEIGHT = 0.45
const SPLAT_HEIGHT = 0.012
const DROP_SPLAT_SCALE = 3.5
const POOL_GROW_SECONDS = 0.8
const DROP_GROW_SECONDS = 0.08
const STREAK = 0.035
const TRAUMA_DECAY = 1.8
const PUNCH_DECAY = 6
const LIGHT_DECAY = 14
const SHAKE_CELLS = 0.18
const SHAKE_ROLL = THREE.MathUtils.degToRad(1.5)
const SHAKE_FREQUENCY = 31
const PUNCH_ZOOM = 0.05
const LIGHT_INTENSITY = 10
const LIGHT_DISTANCE = 4
const SPLAT_EDGE_NOISE = 0.9
const SPLAT_NOISE_SCALE = 1.3
const SPLAT_OPACITY = 0.92

const random = (min: number, max: number) => min + Math.random() * (max - min)

/** The blood, the sparks, the flash of light, the shake and the stop that make a hit felt. */
export class CombatLayer {
  readonly group = new THREE.Group()
  readonly light = new THREE.PointLight(0xffd8a8, 0, LIGHT_DISTANCE, 1)
  private readonly bloodColour = uniform(new THREE.Color())
  private readonly dropMesh: THREE.InstancedMesh
  private readonly sparkMesh: THREE.InstancedMesh
  private readonly splatMesh: THREE.InstancedMesh
  private readonly drops: Particle[] = []
  private readonly sparks: Particle[] = []
  private readonly splats: Splat[] = []
  private nextSplat = 0
  private settings: CombatSettings = { blood: 0x8a0f0f, gore: 1, shake: 0.4, hitStop: 0.07, flash: 1 }
  private trauma = 0
  private punchAmount = 0
  private lightAmount = 0
  private time = 0
  private floor: (x: number, y: number) => boolean = () => false

  constructor() {
    const quad = new THREE.PlaneGeometry(1, 1)
    const dropMaterial = new THREE.MeshBasicNodeMaterial()
    dropMaterial.colorNode = this.bloodColour
    this.dropMesh = new THREE.InstancedMesh(quad, dropMaterial, MAX_DROPS)
    const sparkMaterial = new THREE.MeshBasicNodeMaterial({ color: 0xffe6b0, transparent: true, blending: THREE.AdditiveBlending, depthWrite: false })
    this.sparkMesh = new THREE.InstancedMesh(quad, sparkMaterial, MAX_SPARKS)
    const noise = noiseTexture()
    const edge = length(uv().sub(0.5)).mul(2).add(texture(noise, positionWorld.xz.mul(SPLAT_NOISE_SCALE)).r.sub(0.5).mul(SPLAT_EDGE_NOISE))
    const splatMaterial = new THREE.MeshLambertNodeMaterial({ transparent: true, depthWrite: false })
    splatMaterial.colorNode = this.bloodColour.mul(0.7)
    splatMaterial.opacityNode = edge.smoothstep(0.8, 0.65).mul(SPLAT_OPACITY)
    this.splatMesh = new THREE.InstancedMesh(new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2), splatMaterial, MAX_SPLATS)
    this.splatMesh.renderOrder = 3
    for (const mesh of [this.dropMesh, this.sparkMesh, this.splatMesh]) {
      mesh.count = 0
      mesh.frustumCulled = false
      this.group.add(mesh)
    }
    this.light.castShadow = false
    this.group.add(this.light)
  }

  configure(settings: CombatSettings) {
    this.settings = settings
    this.bloodColour.value.set(settings.blood)
  }

  /**
   * Spill blood and sparks at the cell `at`, away from the attacker at `from`, flash a light and shake the camera.
   * `floor` says on which cells the blood stays. Returns how many seconds the actors stop.
   */
  impact(kind: ImpactKind, at: { x: number; y: number }, from: { x: number; y: number } | undefined, floor: (x: number, y: number) => boolean): number {
    const spec = IMPACTS[kind]
    const { gore, flash, shake, hitStop } = this.settings
    const origin = new THREE.Vector3(at.x + 0.5, IMPACT_HEIGHT, at.y + 0.5)
    const away = from ? new THREE.Vector3(at.x - from.x, 0, at.y - from.y) : new THREE.Vector3(random(-1, 1), 0, random(-1, 1))
    away.normalize()
    for (let i = 0; i < Math.round(spec.drops * gore); i++) {
      const velocity = away.clone().multiplyScalar(random(0.4, 1) * spec.dropSpeed)
      velocity.x += random(-1, 1)
      velocity.z += random(-1, 1)
      velocity.y = random(0.8, 2.6)
      this.add(this.drops, MAX_DROPS, { position: origin.clone(), velocity, age: 0, life: Infinity, size: random(0.035, 0.075) })
    }
    for (let i = 0; i < Math.round(spec.sparks * Math.min(1, flash)); i++) {
      const velocity = new THREE.Vector3(random(-1, 1), random(-0.2, 1), random(-1, 1)).normalize().multiplyScalar(random(2.5, 5.5))
      velocity.addScaledVector(away, -1.5)
      this.add(this.sparks, MAX_SPARKS, { position: origin.clone(), velocity, age: 0, life: random(0.12, 0.28), size: random(0.02, 0.04) })
    }
    if (spec.pool > 0 && gore > 0 && floor(at.x, at.y)) this.splat(origin.x, origin.z, spec.pool * Math.min(1.5, gore), POOL_GROW_SECONDS)
    this.trauma = Math.min(1, this.trauma + spec.trauma * shake)
    this.punchAmount = Math.min(1, this.punchAmount + spec.punch * shake)
    this.lightAmount = spec.light * flash
    this.light.position.copy(origin).setY(IMPACT_HEIGHT + 0.3)
    this.floor = floor
    return hitStop * spec.stop
  }

  /** Move the blood and the sparks on, and turn every particle to face the camera. */
  update(dt: number, camera: THREE.Camera) {
    this.time += dt
    this.trauma = Math.max(0, this.trauma - TRAUMA_DECAY * dt)
    this.punchAmount *= Math.exp(-PUNCH_DECAY * dt)
    this.lightAmount *= Math.exp(-LIGHT_DECAY * dt)
    this.light.intensity = this.lightAmount * LIGHT_INTENSITY
    for (const drop of this.drops) {
      drop.velocity.y -= GRAVITY * dt
      drop.position.addScaledVector(drop.velocity, dt)
      if (drop.position.y > 0) continue
      drop.age = drop.life
      if (this.floor(Math.floor(drop.position.x), Math.floor(drop.position.z))) this.splat(drop.position.x, drop.position.z, drop.size * DROP_SPLAT_SCALE, DROP_GROW_SECONDS)
    }
    for (const spark of this.sparks) {
      spark.age += dt
      spark.velocity.y -= SPARK_GRAVITY * dt
      spark.velocity.multiplyScalar(Math.exp(-3 * dt))
      spark.position.addScaledVector(spark.velocity, dt)
    }
    prune(this.drops)
    prune(this.sparks)
    const right = new THREE.Vector3(1, 0, 0).applyQuaternion(camera.quaternion)
    const up = new THREE.Vector3(0, 1, 0).applyQuaternion(camera.quaternion)
    placeStreaks(this.dropMesh, this.drops, camera.quaternion, right, up, 1)
    placeStreaks(this.sparkMesh, this.sparks, camera.quaternion, right, up, 2.5)
    const matrix = new THREE.Matrix4()
    const rotation = new THREE.Quaternion()
    const axis = new THREE.Vector3(0, 1, 0)
    this.splats.forEach((splat, i) => {
      splat.age += dt
      const grown = splat.size * Math.sqrt(Math.min(1, splat.age / splat.grow))
      rotation.setFromAxisAngle(axis, splat.angle)
      matrix.compose(new THREE.Vector3(splat.x, SPLAT_HEIGHT, splat.z), rotation, new THREE.Vector3(grown, 1, grown * 0.8))
      this.splatMesh.setMatrixAt(i, matrix)
    })
    this.splatMesh.count = this.splats.length
    this.splatMesh.instanceMatrix.needsUpdate = true
  }

  /** How far the camera moves and turns now: `x` and `y` in cells to the screen's right and up, `roll` in radians, and `zoom` to multiply the zoom by. */
  shake(): { x: number; y: number; roll: number; zoom: number } {
    const amount = this.trauma * this.trauma
    const t = this.time * SHAKE_FREQUENCY
    return {
      x: amount * SHAKE_CELLS * (Math.sin(t) * 0.6 + Math.sin(t * 2.3 + 1.7) * 0.4),
      y: amount * SHAKE_CELLS * (Math.sin(t * 1.3 + 0.5) * 0.6 + Math.sin(t * 2.9 + 4.1) * 0.4),
      roll: amount * SHAKE_ROLL * Math.sin(t * 0.9 + 2.2),
      zoom: 1 + this.punchAmount * PUNCH_ZOOM,
    }
  }

  /** Remove the blood on the floor, for a new level. */
  clear() {
    this.drops.length = 0
    this.sparks.length = 0
    this.splats.length = 0
    this.nextSplat = 0
  }

  dispose() {
    for (const mesh of [this.dropMesh, this.sparkMesh, this.splatMesh]) mesh.dispose()
  }

  private add(list: Particle[], max: number, particle: Particle) {
    if (list.length < max) list.push(particle)
  }

  private splat(x: number, z: number, size: number, grow: number) {
    const splat = { x, z, size, angle: random(0, Math.PI * 2), age: 0, grow }
    if (this.splats.length < MAX_SPLATS) this.splats.push(splat)
    else this.splats[this.nextSplat] = splat
    this.nextSplat = (this.nextSplat + 1) % MAX_SPLATS
  }
}

function prune(list: Particle[]) {
  let kept = 0
  for (const p of list) if (p.age < p.life) list[kept++] = p
  list.length = kept
}

/** Every particle is a small square that faces the camera, stretched along the way it moves on the screen. */
function placeStreaks(mesh: THREE.InstancedMesh, list: readonly Particle[], facing: THREE.Quaternion, right: THREE.Vector3, up: THREE.Vector3, stretch: number) {
  const matrix = new THREE.Matrix4()
  const turn = new THREE.Quaternion()
  const rotation = new THREE.Quaternion()
  const forward = new THREE.Vector3(0, 0, 1)
  list.forEach((p, i) => {
    const across = p.velocity.dot(right)
    const along = p.velocity.dot(up)
    const fade = Number.isFinite(p.life) ? 1 - p.age / p.life : 1
    turn.setFromAxisAngle(forward, Math.atan2(along, across))
    rotation.copy(facing).multiply(turn)
    const size = p.size * fade
    matrix.compose(p.position, rotation, new THREE.Vector3(size + Math.hypot(across, along) * STREAK * stretch, size, 1))
    mesh.setMatrixAt(i, matrix)
  })
  mesh.count = list.length
  mesh.instanceMatrix.needsUpdate = true
}
