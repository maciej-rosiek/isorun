import * as THREE from 'three/webgpu'
import { DIR8_DX, DIR8_DY, MEMORY_SHADE, cellKey, getCell, hasItemsUnder, makeCamera, type Camera, type CellKey, type MapRenderer, type Scene, type SceneCell, type SceneCursor, type TileId, type TileSource } from '@isorun/scene'
import { ActorLayer, STRIKE_DELAY, actorId, type UprightPose, type ActorAction, type ActorId, type ActorStep } from './actors.js'
import { CombatLayer } from './combat.js'
import { StepLayer } from './steps.js'
import { KIT_SCALE, type IsoAssets } from './assets.js'
import { LIGHTS, type IsoLights, type PointLightSpec } from './lights.js'
import { QUALITY, type IsoQuality } from './quality.js'
import { IsoPost, overlayOutput } from './post.js'
import { FogLayer } from './fog.js'
import { WaterLayer } from './water.js'
import { AtlasMaterials, capTexture, glowMaterial } from './materials.js'
import { AIM_COLOURS, DEFAULT_VIEW, SIDES, TORCH_HEIGHT, groundBasis, cutAwayFocus, cutAwayKey, groundPoint, hasTorch, isPointable, isShown, planScene, wallHeightAt, tileFrame, type AimIntent, type GroundLayer, type IsoAim, type IsoRoute, type IsoView, type Sprite, type SpriteLayer, type WallBlock } from './plan.js'
import { TileFeatures, isTileFeature } from './tile-features.js'
import { ACTOR_SHADOW_LAYER, ClayMaterial, SHADOW_LAYER, applyShading, castShadow } from './shading.js'
import { ATLAS_INSET, QuadBatch, SPRITE_LEAN, type Point3 } from './quads.js'

interface ChunkPart {
  ground: GroundLayer[]
  walls: WallBlock[]
  sprites: Sprite[]
}

interface StaticChunk {
  key: string
  group: THREE.Group
  geometries: THREE.BufferGeometry[]
  torches: Torch[]
}

interface Torch {
  position: THREE.Vector3
  phase: number
  /** The open cell the torch hangs over. */
  cell: { x: number; y: number }
}

const LIFT_STEP = 0.002
const CHUNK_CELLS = 8
const HEAD_HEIGHT = 1
const SHORE_SEARCH = 4
const MAX_UPRIGHT_STRETCH = 2.5
const TORCH_LIGHTS = 8
/** Torch lights fade out between these distances (cells) from the centre of the view, so none switches on or off at once. */
const TORCH_FADE_START = 6
const TORCH_FADE_END = 9
const SHADOW_MAP_SIZE = 512
const SUN_SHADOW_MAP_SIZE = 2048
/** The sun shines from the left of the screen and a little from the front, this high over the ground. */
const SUN_ELEVATION = THREE.MathUtils.degToRad(55)
const SUN_FRONT = 0.35
const SUN_DISTANCE = 30
const SUN_REACH = 18
const CAMERA_DISTANCE = 60
/** How far in front of and behind the point it looks at the perspective camera draws, in cells. */
const PERSPECTIVE_DEPTH = 60
const CAP_SHADE = 0.6
/** Upright items and features stand this far behind the cell centre, so an actor on the same cell is always in front. */
const BEHIND_ACTORS = 0.1
/** The two copies of a pile's top item that stand behind it, smaller and darker, so a pile reads as more than one thing: across and behind in cells. */
const PILE_COPIES = [
  { across: -0.2, behind: 0.08, mirror: true },
  { across: 0.18, behind: 0.14, mirror: false },
] as const
const PILE_SCALE = 0.75
const PILE_SHADE = 0.55
const ROUTE_STEPS = 64
const ROUTE_DOT_RADIUS = 0.08
const AIM_LIFT = 0.03
/** The mouse snaps to the nearest floor cell within this many cells of where it points, and this many cells on the screen. */
const SNAP_CELLS = 3
const SNAP_SCREEN_CELLS = 1.5
const DANGER_PULSE = 0.3
const DANGER_PULSE_SPEED = 3

interface PendingImpact {
  time: number
  id: ActorId
  action: ActorAction
  from: { x: number; y: number } | undefined
  onLand?: () => void
}

export class IsoRender3d implements MapRenderer {
  private renderer: THREE.WebGPURenderer | null = null
  private post: IsoPost | null = null
  private ready = false
  /** What is drawn; its matrices are brought up to date once a frame, in `render`, not again by each shadow map's render. */
  private readonly world = new THREE.Scene()
  private readonly orthographic = new THREE.OrthographicCamera()
  private readonly perspective = new THREE.PerspectiveCamera()
  private camera: THREE.OrthographicCamera | THREE.PerspectiveCamera = this.orthographic
  private readonly statics = new THREE.Group()
  private readonly chunks = new Map<string, StaticChunk>()
  /** What every chunk was built for: a change builds them all again. */
  private chunksKey = ''
  private chunkMaterials: AtlasMaterials | null = null
  private readonly aimGroup = new THREE.Group()
  private readonly aimMaterial = aimMaterial()
  private readonly aimArrow = new THREE.Mesh(arrowGeometry(), this.aimMaterial)
  private readonly aimSquare = new THREE.Mesh(squareGeometry(), this.aimMaterial)
  private readonly routeGroup = new THREE.Group()
  private readonly routeMaterial = aimMaterial()
  private readonly routeDots = new THREE.InstancedMesh(new THREE.CircleGeometry(ROUTE_DOT_RADIUS, 16).rotateX(-Math.PI / 2), this.routeMaterial, ROUTE_STEPS)
  private readonly routeSquare = new THREE.Mesh(squareGeometry(), this.routeMaterial)
  private readonly cursorMaterial = aimMaterial()
  private readonly cursorSquare = new THREE.Mesh(squareGeometry(), this.cursorMaterial)
  private cursor: SceneCursor | null = null
  private readonly actors = new ActorLayer()
  private readonly glow = glowMaterial()
  private readonly tileTorches: Torch[] = []
  private readonly features = new TileFeatures()
  /** Called for each hop an actor starts. */
  onHop: ((step: ActorStep) => void) | null = null
  private readonly fog = new FogLayer()
  private readonly water = new WaterLayer()
  private readonly combat = new CombatLayer()
  private readonly steps = new StepLayer()
  private readonly impacts: PendingImpact[] = []
  /** The actors' time: it stands still during a hit stop. */
  private clock = 0
  private stopUntil = 0
  private danger = 0
  /** How fast the actors walk and fight: their clock runs this many times as fast as the real one. */
  private animationSpeed = 1.5
  private readonly capMaterial = new ClayMaterial({ map: capTexture(), vertexColors: true })
  private readonly hemisphere = new THREE.HemisphereLight()
  private readonly torchLights = Array.from({ length: TORCH_LIGHTS }, () => new THREE.PointLight())
  /** Torch lights whose shadow map no longer shows what is around them; it is drawn again when the light is on. */
  private readonly staleShadows = new Set(this.torchLights)
  /** The torch each torch light hangs at, null while it has none. */
  private readonly litTorches: (Torch | null)[] = this.torchLights.map(() => null)
  /** The sun's shadow map no longer shows what is under it; it is drawn again on the next frame. */
  private sunStale = true
  private readonly sun = new THREE.DirectionalLight()
  private readonly background = new THREE.Color()
  private paintKey = ''
  private lights: IsoLights = LIGHTS.diorama
  private quality: IsoQuality = QUALITY.medium
  private materials: AtlasMaterials | null = null
  private assets: IsoAssets | null = null
  private tiles: TileSource | null = null
  private scene: Scene | null = null
  private cam: Camera = makeCamera()
  private view: IsoView = DEFAULT_VIEW
  private aim: IsoAim | null = null
  private route: IsoRoute | null = null
  private staticKey = ''
  private sceneRevision = -1
  /** What the level's geometry was last measured against (`staticsRevision`). */
  private measured: { scene: Scene | null; cells: ReadonlyMap<CellKey, SceneCell> | null; standing: string; revision: number } = { scene: null, cells: null, standing: '', revision: 0 }
  private lastFrame = 0
  private width = 1
  private height = 1
  private dpr = 1

  constructor() {
    this.statics.add(this.features.group)
    this.world.add(this.statics, this.actors.group, this.fog.mesh, this.water.mesh, this.combat.group, this.steps.group, this.aimGroup, this.routeGroup, this.cursorSquare, this.hemisphere, this.sun, this.sun.target, ...this.torchLights)
    this.aimGroup.add(this.aimArrow, this.aimSquare)
    this.aimArrow.renderOrder = this.aimSquare.renderOrder = 10
    this.aimGroup.visible = false
    this.routeGroup.add(this.routeDots, this.routeSquare)
    this.routeDots.renderOrder = this.routeSquare.renderOrder = 10
    this.routeDots.frustumCulled = false
    this.routeGroup.visible = false
    this.cursorSquare.renderOrder = 10
    this.cursorSquare.visible = false
    for (const light of this.torchLights) {
      light.shadow.autoUpdate = false
      light.shadow.mapSize.set(SHADOW_MAP_SIZE, SHADOW_MAP_SIZE)
      light.shadow.bias = -0.002
      light.shadow.normalBias = 0.02
      light.shadow.camera.near = 0.05
    }
    this.sun.castShadow = true
    this.sun.shadow.autoUpdate = false
    this.sun.shadow.mapSize.set(SUN_SHADOW_MAP_SIZE, SUN_SHADOW_MAP_SIZE)
    this.sun.shadow.bias = -0.0005
    this.sun.shadow.normalBias = 0.02
    const shadowCamera = this.sun.shadow.camera
    shadowCamera.layers.set(SHADOW_LAYER)
    shadowCamera.layers.enable(ACTOR_SHADOW_LAYER)
    shadowCamera.left = shadowCamera.bottom = -SUN_REACH
    shadowCamera.right = shadowCamera.top = SUN_REACH
    shadowCamera.near = 1
    shadowCamera.far = SUN_DISTANCE * 2
    this.world.background = this.background
    this.world.matrixWorldAutoUpdate = false
  }

  mount(target: HTMLCanvasElement | OffscreenCanvas): void {
    this.renderer = new THREE.WebGPURenderer({ canvas: target, antialias: true })
    this.renderer.outputColorSpace = THREE.SRGBColorSpace
    this.post = new IsoPost(this.renderer, this.world, this.camera)
    this.post.configure(this.lights)
    void this.renderer.init().then(() => (this.ready = true))
    this.renderer.shadowMap.type = THREE.PCFShadowMap
    this.renderer.toneMapping = THREE.NeutralToneMapping
    this.applyLights()
    this.applyQuality()
  }

  setTiles(tiles: TileSource): void {
    this.tiles = tiles
    this.rebuildMaterials()
  }

  /** The 3D models; until they are set, the walls carry no torches. */
  setAssets(assets: IsoAssets): void {
    this.assets = assets
    this.invalidate()
  }

  /** A scene with the same revision as the last one is the same scene: the client hands it over every frame. */
  setScene(scene: Scene): void {
    if (scene === this.scene && scene.revision === this.sceneRevision) return
    this.scene = scene
    this.sceneRevision = scene.revision
    this.syncActors(scene)
  }

  /** The player is on another level: the blood, the dust and the footprints of the last one go. */
  newLevel(): void {
    this.combat.clear()
    this.steps.clear()
  }

  setCamera(cam: Camera): void {
    this.cam = cam
  }

  /** The cursor of an aim or a look: a square on its cell, red while aiming. The level map's cursor is the map's own. */
  setCursor(cursor: SceneCursor | null): void {
    this.cursor = cursor
  }

  setView(view: IsoView): void {
    this.view = view
  }

  setLights(lights: IsoLights): void {
    this.lights = lights
    this.applyLights()
  }

  /** How much work a frame does: the pixel ratio, the ambient occlusion's resolution and the shadows. */
  setQuality(quality: IsoQuality): void {
    this.quality = quality
    this.applyQuality()
  }

  setAim(aim: IsoAim | null): void {
    this.aim = aim
  }

  /** The way the mouse points: dots on the steps, and the square on the target. */
  setRoute(route: IsoRoute | null): void {
    this.route = route
  }

  /** Where the player is drawn now, in cells: between two cells during a hop; null before the player is drawn. */
  playerShown(): { x: number; y: number } | null {
    return this.actors.shownCellOf('player') ?? null
  }

  /** Called when a door of the level opens or shuts; the first build of a level is not a change. */
  set onDoor(listener: ((x: number, y: number, open: boolean) => void) | null) {
    this.features.onDoor = listener
  }

  /** How fast the actors walk and fight, 1 for the base speed. */
  setAnimationSpeed(speed: number): void {
    this.animationSpeed = Math.max(0.1, speed)
  }

  /** How near to death the player is, 0 to 1: the screen edge turns softly red and pulses. */
  setDanger(amount: number): void {
    this.danger = THREE.MathUtils.clamp(amount, 0, 1)
  }

  /**
   * Play an action on the actor with this id: the player is `'player'`, a monster its client id. A hit, a miss
   * or a kill lands when the attack from `target` reaches the actor, a hit or a kill with blood, sparks, a flash
   * and a shake. `onShown` runs when the action shows: an attack's swing forward, the moment a blow lands.
   */
  act(id: ActorId, action: ActorAction, target?: { x: number; y: number }, onShown?: () => void): void {
    if (action === 'attack') {
      this.actors.act(id, action, this.clock, target)
      if (onShown) this.impacts.push({ time: this.clock + STRIKE_DELAY, id, action, from: target, onLand: onShown })
      return
    }
    const time = this.actors.impactFrom(target, this.clock)
    this.actors.act(id, action, time, target)
    this.impacts.push({ time, id, action, from: target, onLand: onShown })
  }

  render(): void {
    const renderer = this.renderer
    const scene = this.scene
    if (!renderer || !this.ready || !scene || !this.tiles || !this.materials) return
    const now = performance.now() / 1000
    const dt = this.lastFrame ? Math.min(0.1, now - this.lastFrame) : 0
    this.lastFrame = now
    const staticKey = `${this.staticsRevision(scene)}|${cutAwayKey(scene, this.focus(scene), this.cam.yaw, -this.cam.pitch, this.view)}|${this.cam.yaw}|${this.cam.pitch}|${this.view.wallHeight}|${JSON.stringify(this.lights.walls)}|${this.lights.fog.height}|${this.lights.fog.reach}|${this.lights.water.enabled}`
    const rebuilt = staticKey !== this.staticKey
    if (rebuilt) {
      this.buildStatics(scene, this.tiles, this.materials)
      this.staticKey = staticKey
    }
    const doorsMoved = this.features.update(dt)
    if (now >= this.stopUntil) this.clock += dt * this.animationSpeed
    this.landImpacts(scene, now)
    const hops = this.actors.update(this.clock, this.cam.yaw, this.upright(), (x, y) => this.features.groundAt(x, y))
    const floor = groundFloor(scene)
    for (const hop of hops) {
      this.steps.step(hop.from, hop.to, hop.seconds / this.animationSpeed, floor)
      this.onHop?.(hop)
    }
    const actorShadowsMoved = this.actors.takeShadowChange()
    if (doorsMoved || rebuilt || (actorShadowsMoved && this.quality.actorTorchShadows)) this.markShadowsStale()
    else if (actorShadowsMoved) this.sunStale = true
    this.fog.update(dt)
    this.placeCamera()
    this.combat.update(dt, this.camera)
    this.steps.update(dt, this.camera)
    this.post?.setDanger(this.danger * (1 - DANGER_PULSE + DANGER_PULSE * Math.sin(now * DANGER_PULSE_SPEED)))
    this.placeLights(now)
    this.placeAim(scene)
    this.placeRoute(scene)
    this.placeCursor(scene)
    this.materials.upload(renderer)
    this.world.updateMatrixWorld()
    const origin = new THREE.Vector3(0, 0, 0).project(this.camera)
    const pixels = this.pixelRatio()
    this.post?.render({ scale: (this.view.zoom / DEFAULT_VIEW.zoom) * pixels, x: ((origin.x + 1) / 2) * this.width * pixels, y: ((1 - origin.y) / 2) * this.height * pixels }, this.lights)
  }

  /** The floor cell under a point of the canvas, in CSS pixels; over a wall, the fog or the void, the floor cell nearest to the point on the screen. */
  pick(px: number, py: number): CellKey | null {
    const scene = this.scene
    if (!scene) return null
    this.placeCamera()
    this.camera.updateMatrixWorld()
    const origin = new THREE.Vector3((px / this.width) * 2 - 1, -(py / this.height) * 2 + 1, -1).unproject(this.camera)
    const direction = new THREE.Vector3((px / this.width) * 2 - 1, -(py / this.height) * 2 + 1, 1).unproject(this.camera).sub(origin).normalize()
    const ground = groundPoint(origin, direction)
    if (!ground) return null
    const gx = Math.floor(ground.x)
    const gy = Math.floor(ground.y)
    if (isPointable(getCell(scene, gx, gy))) return cellKey(gx, gy)
    let best: { key: CellKey; distance: number } | null = null
    const onScreen = new THREE.Vector3()
    for (let y = gy - SNAP_CELLS; y <= gy + SNAP_CELLS; y++)
      for (let x = gx - SNAP_CELLS; x <= gx + SNAP_CELLS; x++) {
        if (!isPointable(getCell(scene, x, y))) continue
        onScreen.set(x + 0.5, 0, y + 0.5).project(this.camera)
        const distance = Math.hypot(((onScreen.x + 1) / 2) * this.width - px, ((1 - onScreen.y) / 2) * this.height - py)
        if (!best || distance < best.distance) best = { key: cellKey(x, y), distance }
      }
    return best && best.distance <= SNAP_SCREEN_CELLS * this.view.zoom ? best.key : null
  }

  /** Where the middle of a cell, at head height, is on the canvas, in CSS pixels. */
  screenOf(x: number, y: number): { x: number; y: number } {
    const p = new THREE.Vector3(x + 0.5, HEAD_HEIGHT, y + 0.5).project(this.camera)
    return { x: ((p.x + 1) / 2) * this.width, y: ((1 - p.y) / 2) * this.height }
  }

  resize(width: number, height: number, dpr: number): void {
    this.width = Math.max(1, width)
    this.height = Math.max(1, height)
    this.dpr = dpr
    this.applySize()
  }

  destroy(): void {
    this.clearStatics()
    this.actors.clear()
    this.combat.dispose()
    this.steps.dispose()
    this.materials?.dispose()
    this.fog.dispose()
    this.water.dispose()
    this.post?.dispose()
    this.renderer?.dispose()
    this.renderer = null
  }

  private rebuildMaterials() {
    if (!this.tiles) return
    this.materials?.dispose()
    this.materials = new AtlasMaterials(this.tiles, this.lights.paint)
    this.paintKey = JSON.stringify(this.lights.paint)
    this.invalidate()
  }

  private invalidate() {
    this.staticKey = ''
    if (this.scene) this.syncActors(this.scene)
  }

  private syncActors(scene: Scene) {
    if (!this.tiles || !this.materials) return
    this.actors.sync(scene, this.clock, { tiles: this.tiles, materials: this.materials })
  }

  private landImpacts(scene: Scene, now: number) {
    const floor = groundFloor(scene)
    for (let i = 0; i < this.impacts.length; ) {
      const impact = this.impacts[i]
      if (impact.time > this.clock) {
        i++
        continue
      }
      this.impacts.splice(i, 1)
      impact.onLand?.()
      if (impact.action === 'attack' || impact.action === 'miss') continue
      const at = this.actors.cellOf(impact.id)
      if (!at) continue
      const kind = impact.action === 'death' ? 'death' : impact.id === 'player' ? 'hurt' : 'hit'
      this.stopUntil = Math.max(this.stopUntil, now + this.combat.impact(kind, at, impact.from, floor))
    }
  }

  /** How upright sprites stand: stretched up so the view's angle does not flatten them, and, when the camera is too high for that, leaned back toward it. */
  private upright(): UprightPose {
    const elevation = -this.cam.pitch
    return { stretch: Math.min(MAX_UPRIGHT_STRETCH, 1 / Math.max(0.01, Math.cos(elevation))), lean: Math.max(0, elevation - Math.acos(1 / MAX_UPRIGHT_STRETCH)) }
  }

  private applyLights() {
    const { ambient } = this.lights
    this.hemisphere.color.set(ambient.sky)
    this.hemisphere.groundColor.set(ambient.ground)
    this.hemisphere.intensity = ambient.intensity
    this.background.set(this.lights.background)
    this.sun.color.set(this.lights.sun.colour)
    this.sun.intensity = this.lights.sun.intensity
    this.sun.castShadow = this.lights.sun.intensity > 0
    this.sun.shadow.radius = this.lights.shadowSoftness
    applyShading(this.lights.shading)
    if (JSON.stringify(this.lights.paint) !== this.paintKey) this.rebuildMaterials()
    for (const light of this.torchLights) applyPointLight(light, this.lights.torch)
    for (const light of this.torchLights) light.shadow.radius = this.lights.shadowSoftness
    this.markShadowsStale()
    if (this.renderer) this.renderer.toneMappingExposure = this.lights.exposure
    this.fog.configure(this.lights.fog)
    this.water.configure(this.lights.water)
    this.combat.configure(this.lights.combat)
    this.steps.configure(this.lights.steps)
    this.post?.configure(this.lights)
  }

  private applyQuality() {
    this.torchLights.forEach((light, i) => {
      light.castShadow = i < this.quality.torchShadows
      light.shadow.camera.layers.set(SHADOW_LAYER)
      if (this.quality.actorTorchShadows) light.shadow.camera.layers.enable(ACTOR_SHADOW_LAYER)
    })
    this.markShadowsStale()
    if (this.renderer) this.renderer.shadowMap.enabled = this.quality.shadows
    this.post?.setAoResolution(this.quality.aoResolution)
    this.applySize()
  }

  private markShadowsStale() {
    for (const light of this.torchLights) this.staleShadows.add(light)
    this.sunStale = true
  }

  private pixelRatio() {
    return Math.min(this.dpr, this.quality.pixelRatio)
  }

  private applySize() {
    if (!this.renderer) return
    this.renderer.setPixelRatio(this.pixelRatio())
    this.renderer.setSize(this.width, this.height, false)
  }

  private placeCamera() {
    const elevation = -this.cam.pitch
    const { right, forward } = groundBasis(this.cam.yaw)
    const shake = this.combat.shake()
    const target = new THREE.Vector3(this.cam.eyeX + 0.5, 0, this.cam.eyeY + 0.5)
    target.x += right.x * shake.x + forward.x * shake.y
    target.z += right.y * shake.x + forward.y * shake.y
    const halfWidth = this.width / 2 / this.view.zoom / shake.zoom
    const halfHeight = this.height / 2 / this.view.zoom / shake.zoom
    const fov = this.view.fov ?? 0
    const camera = fov > 0 ? this.perspective : this.orthographic
    const distance = fov > 0 ? halfHeight / Math.tan(THREE.MathUtils.degToRad(fov) / 2) : CAMERA_DISTANCE
    camera.position.set(
      target.x - forward.x * Math.cos(elevation) * distance,
      Math.sin(elevation) * distance,
      target.z - forward.y * Math.cos(elevation) * distance,
    )
    camera.up.set(0, 1, 0)
    camera.lookAt(target)
    camera.rotateZ(shake.roll)
    if (camera instanceof THREE.PerspectiveCamera) {
      camera.fov = fov
      camera.aspect = this.width / this.height
      camera.near = Math.max(0.1, distance - PERSPECTIVE_DEPTH)
      camera.far = distance + PERSPECTIVE_DEPTH
    } else {
      camera.left = -halfWidth
      camera.right = halfWidth
      camera.top = halfHeight
      camera.bottom = -halfHeight
      camera.near = 0.1
      camera.far = CAMERA_DISTANCE * 2
    }
    camera.updateProjectionMatrix()
    camera.updateMatrixWorld()
    if (camera !== this.camera) {
      this.camera = camera
      this.post?.setCamera(camera)
    }
    this.fog.view(camera)
  }

  /**
   * The sun follows the camera a whole cell at a time, so its shadow map is drawn again only when it moves or
   * what casts a shadow changes. The torch lights hang at the torches nearest to the camera, the nearest with
   * shadows; a light keeps its torch while the torch is still among them. A torch shadow is six renders of
   * the world, so one of them is drawn again a frame, and a light that goes to another torch waits at its
   * last one until its shadow is drawn there.
   */
  private placeLights(now: number) {
    const { right, forward } = groundBasis(this.cam.yaw)
    const target = new THREE.Vector3(Math.round(this.cam.eyeX) + 0.5, 0, Math.round(this.cam.eyeY) + 0.5)
    const across = new THREE.Vector3(-right.x - forward.x * SUN_FRONT, 0, -right.y - forward.y * SUN_FRONT).normalize().multiplyScalar(Math.cos(SUN_ELEVATION))
    const sunAt = target.clone().addScaledVector(across.setY(Math.sin(SUN_ELEVATION)), SUN_DISTANCE)
    if (this.sunStale || !this.sun.position.equals(sunAt)) {
      this.sun.target.position.copy(target)
      this.sun.position.copy(sunAt)
      this.sun.shadow.needsUpdate = this.sun.castShadow
      this.sunStale = false
    }
    const from = new THREE.Vector3(this.cam.eyeX + 0.5, 1, this.cam.eyeY + 0.5)
    const fade = (torch: Torch) => 1 - THREE.MathUtils.smoothstep(Math.hypot(torch.position.x - from.x, torch.position.z - from.z), TORCH_FADE_START, TORCH_FADE_END)
    const nearest = [...this.tileTorches].sort((a, b) => a.position.distanceToSquared(from) - b.position.distanceToSquared(from)).slice(0, TORCH_LIGHTS)
    const shadows = this.torchLights.filter((light) => light.castShadow).length
    const wanted = assignTorches(this.litTorches.slice(0, shadows), nearest.slice(0, shadows))
    const due = wanted
      .map((torch, i) => ({ torch, i, moves: !sameTorch(torch, this.litTorches[i]) }))
      .filter(({ torch, i, moves }) => torch !== null && fade(torch) > 0 && (moves || this.staleShadows.has(this.torchLights[i])))
      .sort((a, b) => Number(b.moves) - Number(a.moves) || a.i - b.i)[0]
    const shadowed = wanted.map((torch, i) => (i === due?.i || sameTorch(torch, this.litTorches[i]) ? torch : this.litTorches[i]))
    const unshadowed = assignTorches(this.litTorches.slice(shadows), nearest.filter((torch) => !shadowed.some((held) => sameTorch(held, torch))))
    const placed = [...shadowed, ...unshadowed]
    this.torchLights.forEach((light, i) => {
      const torch = placed[i]
      this.litTorches[i] = torch
      if (!torch) {
        light.intensity = 0
        return
      }
      if (!light.position.equals(torch.position)) this.staleShadows.add(light)
      light.position.copy(torch.position)
      light.intensity = fade(torch) * this.lights.torch.intensity * (0.85 + 0.1 * Math.sin(now * 13 + torch.phase) + 0.05 * Math.sin(now * 29 + torch.phase * 2))
      if (i === due?.i && this.staleShadows.delete(light)) light.shadow.needsUpdate = true
    })
  }

  private placeAim(scene: Scene) {
    const aim = this.aim
    this.aimGroup.visible = aim !== null && scene.playerOnLevel
    if (!aim) return
    this.aimMaterial.color.copy(this.aimColour(aim.intent))
    const dx = DIR8_DX[aim.dir]
    const dy = DIR8_DY[aim.dir]
    this.aimGroup.position.set(scene.player.x + 0.5, AIM_LIFT, scene.player.y + 0.5)
    const angle = Math.atan2(dy, dx)
    const reach = Math.hypot(dx, dy)
    this.aimGroup.rotation.set(0, -angle, 0)
    this.aimArrow.scale.set(reach, 1, 1)
    this.aimSquare.position.set(reach, 0, 0)
    this.aimSquare.rotation.set(0, angle, 0)
  }

  /**
   * A count that goes up when the scene changes what the level's geometry is built from: a cell, an item
   * or feature standing on one, or who stands in a doorway. A monster that moves leaves it as it is.
   */
  private staticsRevision(scene: Scene): number {
    const measured = this.measured
    if (scene === measured.scene) return measured.revision
    const standing = staticsStanding(scene)
    const changed = standing !== measured.standing || !sameCells(measured.cells, scene.cells)
    this.measured = { scene, cells: scene.cells, standing, revision: measured.revision + (changed ? 1 : 0) }
    return this.measured.revision
  }

  private focus(scene: Scene) {
    const target = this.route?.target ?? (this.cursor && this.cursor.mode !== 'map' ? this.cursor : undefined)
    const onFloor = target !== undefined && isPointable(getCell(scene, target.x, target.y))
    return cutAwayFocus(scene, onFloor ? target : null)
  }

  private aimColour(intent: AimIntent) {
    return new THREE.Color(intent === 'move' ? this.lights.aim.colour : AIM_COLOURS[intent]).multiplyScalar(this.lights.aim.brightness)
  }

  private placeRoute(scene: Scene) {
    const route = this.route
    this.routeGroup.visible = route !== null
    if (!route) return
    this.routeMaterial.color.copy(this.aimColour(route.intent))
    const matrix = new THREE.Matrix4()
    const steps = route.steps.slice(0, ROUTE_STEPS)
    steps.forEach((s, i) => this.routeDots.setMatrixAt(i, matrix.makeTranslation(s.x + 0.5, AIM_LIFT, s.y + 0.5)))
    this.routeDots.count = steps.length
    this.routeDots.instanceMatrix.needsUpdate = true
    const onWall = !isPointable(getCell(scene, route.target.x, route.target.y))
    const lift = onWall ? wallHeightAt(route.target.x, route.target.y, this.focus(scene), this.cam.yaw, -this.cam.pitch, this.view) + AIM_LIFT : AIM_LIFT
    this.routeSquare.position.set(route.target.x + 0.5, lift, route.target.y + 0.5)
  }

  private placeCursor(scene: Scene) {
    const cursor = this.cursor
    this.cursorSquare.visible = cursor !== null && cursor.mode !== 'map'
    if (!cursor) return
    this.cursorMaterial.color.copy(this.aimColour(cursor.mode === 'target' ? 'attack' : 'move'))
    const onWall = !isPointable(getCell(scene, cursor.x, cursor.y))
    const lift = onWall ? wallHeightAt(cursor.x, cursor.y, this.focus(scene), this.cam.yaw, -this.cam.pitch, this.view) + AIM_LIFT : AIM_LIFT
    this.cursorSquare.position.set(cursor.x + 0.5, lift, cursor.y + 0.5)
  }

  private clearStatics() {
    for (const chunk of this.chunks.values()) this.dropChunk(chunk)
    this.chunks.clear()
    this.chunksKey = ''
    this.tileTorches.length = 0
  }

  private dropChunk(chunk: StaticChunk) {
    for (const g of chunk.geometries) g.dispose()
    this.statics.remove(chunk.group)
  }

  private addTileTorch(chunk: StaticChunk, x: number, y: number, normal: { x: number; y: number }, wallHeight: number) {
    if (!this.assets) return
    const height = TORCH_HEIGHT * Math.min(1, wallHeight / 0.85)
    const torch = this.assets.torch()
    torch.scale.setScalar(KIT_SCALE)
    torch.position.set(x + 0.5 + normal.x * 0.5, height, y + 0.5 + normal.y * 0.5)
    torch.rotation.y = Math.atan2(normal.x, normal.y)
    const glow = new THREE.Sprite(this.glow)
    glow.position.set(x + 0.5 + normal.x * 0.62, height + 0.2, y + 0.5 + normal.y * 0.62)
    glow.scale.setScalar(0.45)
    chunk.group.add(torch, glow)
    chunk.torches.push({ position: new THREE.Vector3(x + 0.5 + normal.x * 0.75, height + 0.2, y + 0.5 + normal.y * 0.75), phase: (x * 7 + y * 13) % 10, cell: { x: x + normal.x, y: y + normal.y } })
  }

  /** The level's geometry in chunks of `CHUNK_CELLS` square: only a chunk whose cells, cut-away walls or items changed is built again. */
  private buildStatics(scene: Scene, tiles: TileSource, materials: AtlasMaterials) {
    const { stretch, lean } = this.upright()
    const chunksKey = `${this.cam.yaw}|${stretch}|${lean}|${this.view.wallHeight}|${JSON.stringify(this.lights.walls)}|${this.lights.water.enabled}|${this.assets !== null}`
    if (chunksKey !== this.chunksKey || materials !== this.chunkMaterials) {
      this.clearStatics()
      this.chunksKey = chunksKey
      this.chunkMaterials = materials
    }
    const plan = planScene(scene, this.cam.yaw, -this.cam.pitch, this.view, this.focus(scene))
    this.fog.build(scene, this.lights.fog.height, this.lights.fog.reach)
    this.water.build(scene)
    this.features.build(scene, tiles, materials, this.view.wallHeight, groundBasis(this.cam.yaw).forward)
    const parts = new Map<string, ChunkPart>()
    const partAt = (x: number, y: number) => {
      const id = `${Math.floor(x / CHUNK_CELLS)},${Math.floor(y / CHUNK_CELLS)}`
      let part = parts.get(id)
      if (!part) parts.set(id, (part = { ground: [], walls: [], sprites: [] }))
      return part
    }
    for (const g of plan.ground) if (!isTileFeature(getCell(scene, g.x, g.y))) partAt(g.x, g.y).ground.push(g)
    for (const w of plan.walls) if (!isTileFeature(getCell(scene, w.x, w.y))) partAt(w.x, w.y).walls.push(w)
    for (const sprite of plan.sprites) {
      const b = sprite.billboard
      if (b && actorId(b) !== undefined) continue
      if (!b && isTileFeature(getCell(scene, sprite.x, sprite.y))) continue
      partAt(sprite.x, sprite.y).sprites.push(sprite)
    }
    const shadeAt = (x: number, y: number) => (getCell(scene, x, y)?.visibility === 'remembered' ? MEMORY_SHADE : 1)
    for (const [id, part] of parts) {
      const key = JSON.stringify([
        part.ground.map((g) => [g.x, g.y, this.groundTile(scene, g), g.lift, shadeAt(g.x, g.y)]),
        part.walls.map((w) => [w.x, w.y, w.height, w.top, w.faces, shadeAt(w.x, w.y)]),
        part.sprites.map((sprite) => [sprite.x, sprite.y, sprite.lying, sprite.pile, sprite.layers, shadeAt(sprite.x, sprite.y)]),
      ])
      const old = this.chunks.get(id)
      if (old?.key === key) continue
      if (old) this.dropChunk(old)
      const chunk = this.buildChunk(scene, tiles, materials, part, key)
      this.chunks.set(id, chunk)
      this.statics.add(chunk.group)
    }
    for (const [id, chunk] of this.chunks)
      if (!parts.has(id)) {
        this.dropChunk(chunk)
        this.chunks.delete(id)
      }
    this.tileTorches.length = 0
    for (const chunk of this.chunks.values()) this.tileTorches.push(...chunk.torches)
  }

  /** The floor tile a ground layer shows: under drawn water, the nearest land's floor. */
  private groundTile(scene: Scene, g: GroundLayer): TileId {
    const underWater = this.lights.water.enabled && getCell(scene, g.x, g.y)?.flags.water === true
    return g.lift === 0 && underWater ? (shoreFloor(scene, g.x, g.y) ?? g.tile) : g.tile
  }

  private buildChunk(scene: Scene, tiles: TileSource, materials: AtlasMaterials, part: ChunkPart, key: string): StaticChunk {
    const chunk: StaticChunk = { key, group: new THREE.Group(), geometries: [], torches: [] }
    const groundBatches = new Map<string, QuadBatch>()
    const spriteBatches = new Map<string, QuadBatch>()
    let batches = groundBatches
    const shadeAt = (x: number, y: number) => (getCell(scene, x, y)?.visibility === 'remembered' ? MEMORY_SHADE : 1)
    const quad = (tile: TileId, corners: [Point3, Point3, Point3, Point3], texelHeight: number | undefined, shade: number) => {
      const rect = tiles.tile(tile)
      if (!rect) return
      const placed = materials.forTile(rect)
      if (!placed) return
      let batch = batches.get(placed.rect.atlas)
      if (!batch) batches.set(placed.rect.atlas, (batch = new QuadBatch(placed.width, placed.height, batches === spriteBatches ? SPRITE_LEAN : 0, ATLAS_INSET)))
      batch.add(corners, placed.rect.sx, placed.rect.sy, rect.w, texelHeight ?? rect.h, shade)
    }

    const caps = new QuadBatch(1, 1)
    const moss = new THREE.Color(this.lights.walls.mossColour)
    const mossAmount = this.lights.walls.moss
    for (const g of part.ground) {
      const y = g.lift * LIFT_STEP
      quad(this.groundTile(scene, g), [[g.x, y, g.y], [g.x + 1, y, g.y], [g.x + 1, y, g.y + 1], [g.x, y, g.y + 1]], undefined, shadeAt(g.x, g.y))
    }
    for (const w of part.walls) {
      const h = w.height
      const topRect = tiles.tile(w.top)
      const [r, g, b] = topRect ? materials.averageColour(topRect) : [0.3, 0.3, 0.3]
      const shade = shadeAt(w.x, w.y)
      const capColour: Point3 = [THREE.MathUtils.lerp(r * CAP_SHADE, moss.r, mossAmount) * shade, THREE.MathUtils.lerp(g * CAP_SHADE, moss.g, mossAmount) * shade, THREE.MathUtils.lerp(b * CAP_SHADE, moss.b, mossAmount) * shade]
      const bevel = Math.min(this.lights.walls.bevel, h / 2)
      const exposed = new Set(w.faces.map((f) => f.side))
      const x0 = w.x + (exposed.has('w') ? bevel : 0)
      const x1 = w.x + 1 - (exposed.has('e') ? bevel : 0)
      const z0 = w.y + (exposed.has('n') ? bevel : 0)
      const z1 = w.y + 1 - (exposed.has('s') ? bevel : 0)
      caps.add([[x0, h, z0], [x1, h, z0], [x1, h, z1], [x0, h, z1]], 0, 0, 1, 1, capColour)
      const faceTop = h - bevel
      for (const face of w.faces) {
        const { from, to } = SIDES[face.side]
        const ax = w.x + from.x
        const ay = w.y + from.y
        const bx = w.x + to.x
        const by = w.y + to.y
        const rect = tiles.tile(face.tile)
        const texelHeight = rect ? rect.h * Math.min(1, faceTop) : undefined
        quad(face.tile, [[ax, faceTop, ay], [bx, faceTop, by], [bx, 0, by], [ax, 0, ay]], texelHeight, shade)
        if (bevel > 0) {
          const clampX = (x: number) => THREE.MathUtils.clamp(x, x0, x1)
          const clampZ = (z: number) => THREE.MathUtils.clamp(z, z0, z1)
          caps.add([[clampX(ax), h, clampZ(ay)], [clampX(bx), h, clampZ(by)], [bx, faceTop, by], [ax, faceTop, ay]], 0, 0, 1, 1, capColour)
        }
        if (h >= this.view.wallHeight && hasTorch(w.x, w.y, face.side)) this.addTileTorch(chunk, w.x, w.y, SIDES[face.side].normal, h)
      }
    }

    const { right, forward } = groundBasis(this.cam.yaw)
    const { stretch, lean } = this.upright()
    batches = spriteBatches
    const lyingLayer = (s: Sprite, layer: SpriteLayer, i: number, copy: { across: number; behind: number; mirror: boolean } | null) => {
      const rect = tiles.tile(layer.tile)
      if (!rect) return
      const f = tileFrame(rect, layer.ox, layer.oy, layer.ymax)
      if (f.height <= 0) return
      const scale = copy ? PILE_SCALE : 1
      const y = (copy ? 4 + i : 10 + i) * LIFT_STEP
      const cx = s.x + 0.5 + (copy ? right.x * copy.across + forward.x * copy.behind : 0)
      const cy = s.y + 0.5 + (copy ? right.y * copy.across + forward.y * copy.behind : 0)
      const x0 = cx + (f.left - 0.5) * scale
      const y0 = cy + (f.top - 0.5) * scale
      const x1 = x0 + f.width * scale
      const y1 = y0 + f.height * scale
      const [l, r] = copy?.mirror ? [x1, x0] : [x0, x1]
      quad(layer.tile, [[l, y, y0], [r, y, y0], [r, y, y1], [l, y, y1]], f.texelHeight, shadeAt(s.x, s.y) * (copy ? PILE_SHADE : 1))
    }
    const standingLayer = (s: Sprite, layer: SpriteLayer, copy: { across: number; behind: number; mirror: boolean } | null) => {
      const rect = tiles.tile(layer.tile)
      if (!rect) return
      const f = tileFrame(rect, layer.ox, layer.oy, layer.ymax)
      if (f.height <= 0) return
      const scale = copy ? PILE_SCALE : 1
      const cx = s.x + 0.5 + forward.x * (BEHIND_ACTORS + (copy?.behind ?? 0)) + right.x * (copy?.across ?? 0)
      const cy = s.y + 0.5 + forward.y * (BEHIND_ACTORS + (copy?.behind ?? 0)) + right.y * (copy?.across ?? 0)
      const left = (f.left - 0.5) * scale
      const rightEdge = left + f.width * scale
      const top = (1 - f.top) * stretch * scale
      const bottom = (1 - f.top - f.height) * stretch * scale
      const [l, r] = copy?.mirror ? [rightEdge, left] : [left, rightEdge]
      const at = (across: number, up: number): Point3 => [cx + right.x * across + forward.x * up * Math.sin(lean), up * Math.cos(lean), cy + right.y * across + forward.y * up * Math.sin(lean)]
      quad(layer.tile, [at(l, top), at(r, top), at(r, bottom), at(l, bottom)], f.texelHeight, shadeAt(s.x, s.y) * (copy ? PILE_SHADE : 1))
    }
    for (const s of part.sprites) {
      const copies = s.pile ? PILE_COPIES : []
      for (const copy of copies) s.layers.forEach((layer, i) => (s.lying ? lyingLayer(s, layer, i, copy) : standingLayer(s, layer, copy)))
      s.layers.forEach((layer, i) => (s.lying ? lyingLayer(s, layer, i, null) : standingLayer(s, layer, null)))
    }

    if (caps.positions.length) {
      const geometry = caps.geometry()
      chunk.geometries.push(geometry)
      const mesh = new THREE.Mesh(geometry, this.capMaterial)
      mesh.receiveShadow = true
      chunk.group.add(mesh)
    }
    for (const [group, sprites] of [[groundBatches, false], [spriteBatches, true]] as const) {
      for (const [atlas, batch] of group) {
        const entry = materials.material(atlas)
        if (!entry) continue
        const geometry = batch.geometry()
        chunk.geometries.push(geometry)
        const mesh = new THREE.Mesh(geometry, sprites ? entry.sprite : entry.material)
        mesh.receiveShadow = !sprites
        castShadow(mesh, sprites)
        chunk.group.add(mesh)
      }
    }
    chunk.group.updateMatrixWorld(true)
    chunk.group.matrixWorldAutoUpdate = false
    return chunk
  }
}

function applyPointLight(light: THREE.PointLight, spec: PointLightSpec) {
  light.color.set(spec.colour)
  light.intensity = spec.intensity
  light.distance = spec.distance
  light.decay = spec.decay
  light.shadow.camera.far = spec.distance
}

function aimMaterial() {
  const material = new THREE.MeshBasicNodeMaterial({ color: AIM_COLOURS.move, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2 })
  material.mrtNode = overlayOutput()
  return material
}

/** The floor tile of the nearest land to a water cell, laid under the drawn water. */
function shoreFloor(scene: Scene, x: number, y: number): TileId | undefined {
  for (let r = 1; r <= SHORE_SEARCH; r++)
    for (let dy = -r; dy <= r; dy++)
      for (let dx = -r; dx <= r; dx++) {
        const cell = getCell(scene, x + dx, y + dy)
        if (cell && !cell.occluder && !cell.flags.water) return cell.floorTile
      }
  return undefined
}

/** An arrow from the player's centre along +x, one cell long, lying on the ground. */
function arrowGeometry(): THREE.BufferGeometry {
  const shape = new THREE.Shape()
  shape.moveTo(0.18, -0.05)
  shape.lineTo(0.62, -0.05)
  shape.lineTo(0.62, -0.13)
  shape.lineTo(0.8, 0)
  shape.lineTo(0.62, 0.13)
  shape.lineTo(0.62, 0.05)
  shape.lineTo(0.18, 0.05)
  shape.closePath()
  return new THREE.ShapeGeometry(shape).rotateX(Math.PI / 2)
}

/** A square frame around the target cell, lying on the ground. */
function squareGeometry(): THREE.BufferGeometry {
  const outer = 0.47
  const inner = 0.4
  const shape = new THREE.Shape()
  shape.moveTo(-outer, -outer)
  shape.lineTo(outer, -outer)
  shape.lineTo(outer, outer)
  shape.lineTo(-outer, outer)
  shape.closePath()
  const hole = new THREE.Path()
  hole.moveTo(-inner, -inner)
  hole.lineTo(-inner, inner)
  hole.lineTo(inner, inner)
  hole.lineTo(inner, -inner)
  hole.closePath()
  shape.holes.push(hole)
  return new THREE.ShapeGeometry(shape).rotateX(Math.PI / 2)
}

/** Whether blood, dust and footprints show on a cell: open dry ground in sight. */
function groundFloor(scene: Scene) {
  return (x: number, y: number) => {
    const cell = getCell(scene, x, y)
    return cell !== undefined && !cell.occluder && !cell.flags.water && cell.visibility === 'visible'
  }
}

/** Whether two scenes' cells are the same: the same objects, or the same content where the builder made new ones. */
/** The fields of a cell the 3D view does not draw: the text view's glyph, the reticle's words, the minimap's colour, the flash and the cursor marks. */
const UNDRAWN_CELL_FIELDS = new Set(['glyph', 'colour', 'label', 'mf', 'flash', 'cursor', 'cursorKind', 'outOfRange'])

/** Whether two cell maps build the same geometry: a cell that only changed in a field the 3D view does not draw, as its glyph when someone steps on it, is the same. */
function sameCells(before: ReadonlyMap<CellKey, SceneCell> | null, after: ReadonlyMap<CellKey, SceneCell>): boolean {
  if (!before || before.size !== after.size) return false
  for (const [key, cell] of after) {
    const old = before.get(key)
    if (old !== cell && (!old || drawnCell(old) !== drawnCell(cell))) return false
  }
  return true
}

function drawnCell(cell: SceneCell): string {
  return JSON.stringify(cell, (field, value) => (UNDRAWN_CELL_FIELDS.has(field) ? undefined : value))
}

/** What stands in the level's geometry: the items and features that are not actors, and the actors in doorways, which lift the door's lintel. */
function staticsStanding(scene: Scene): string {
  const parts: string[] = []
  for (const b of scene.billboards) {
    if (actorId(b) === undefined) {
      if (isShown(b, scene)) parts.push(`${b.kind}:${b.x},${b.y},${b.tile},${b.lying ? 1 : 0},${hasItemsUnder(b) ? 1 : 0},${b.layers?.map((l) => `${l.tile}/${l.ox ?? 0}/${l.oy ?? 0}/${l.ymax ?? ''}`).join(';') ?? ''}`)
    } else if (getCell(scene, b.x, b.y)?.feature?.type === 'door') parts.push(`door:${b.x},${b.y}`)
  }
  return parts.join('|')
}

function sameTorch(a: Torch | null, b: Torch | null): boolean {
  return a !== null && b !== null && a.position.equals(b.position)
}

/** Which torch each light of a group hangs at: a light keeps its torch while it is still wanted, and the lights set free take the wanted torches left, nearest first. */
function assignTorches(held: readonly (Torch | null)[], wanted: readonly Torch[]): (Torch | null)[] {
  const kept = held.map((torch) => wanted.find((w) => sameTorch(w, torch)) ?? null)
  const left = wanted.filter((w) => !kept.some((k) => sameTorch(k, w)))
  return kept.map((k) => k ?? left.shift() ?? null)
}
