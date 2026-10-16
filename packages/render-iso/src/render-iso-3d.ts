import * as THREE from 'three/webgpu'
import { texture } from 'three/tsl'
import { DIR8_DX, DIR8_DY, cellKey, getCell, keyToXY, hasItemsUnder, makeCamera, type Camera, type CellKey, type MapRenderer, type Scene, type SceneCell, type SceneCursor, type TileId, type TileSource } from '@isorun/scene'
import { ActorLayer, STRIKE_DELAY, actorId, type UprightPose, type ActorAction, type ActorId, type ActorStep } from './actors.js'
import { CombatLayer } from './combat.js'
import { StepLayer } from './steps.js'
import { KIT_SCALE, type IsoAssets } from './assets.js'
import { TorchDust } from './dust.js'
import { TorchLayer, type TorchPlacement } from './torches.js'
import { MeshPool } from './mesh-pool.js'
import { LIGHTS, type IsoLights, type PointLightSpec } from './lights.js'
import { QUALITY, type IsoQuality } from './quality.js'
import { IsoPost, overlayOutput } from './post.js'
import { FogLayer } from './fog.js'
import { FILMS, WaterLayer } from './water.js'
import { StarwaterLayer } from './starwater.js'
import { LavaLayer } from './lava.js'
import { AtlasMaterials, capTexture } from './materials.js'
import { AIM_COLOURS, DEFAULT_VIEW, SIDES, TORCH_HEIGHT, groundBasis, cutAwayCells, cutAwayFocus, cutAwayKey, groundPoint, hasTorch, isPointable, isShown, planScene, wallHeightAt, tileFrame, type AimIntent, type GroundLayer, type IsoAim, type IsoRoute, type IsoView, type Sprite, type SpriteLayer, type Vec2, type WallBlock } from './plan.js'
import { TileFeatures, isTileFeature } from './tile-features.js'
import { ACTOR_SHADOW_LAYER, ClayMaterial, MemoryMask, SHADOW_LAYER, applyShading, castShadow, glowShown, remembered } from './shading.js'
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
  /** Where the chunk's torches are drawn (`TorchLayer`). */
  placements: TorchPlacement[]
  /** Whether a tile of the chunk glows (its label's `glow`), so the screen blooms while the chunk is in view. */
  glows: boolean
}

/** Cells from `x0`, `y0` up to (not with) `x1`, `y1`. */
interface CellArea {
  x0: number
  y0: number
  x1: number
  y1: number
}

const EVERYWHERE: CellArea = { x0: -Infinity, y0: -Infinity, x1: Infinity, y1: Infinity }

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
  /** The ground, the walls and the wall tops of each chunk. */
  private readonly chunks = new Map<string, StaticChunk>()
  /** What stands in each chunk (items, statues), which faces the camera: a turn of the view builds these alone again. */
  private readonly spriteChunks = new Map<string, StaticChunk>()
  /** The view the sprite chunks were built for. */
  private spritesView = ''
  private cutAwayInputs = ''
  private cutAwayCache = ''
  /** What every chunk was built for: a change builds them all again. */
  private chunksKey = ''
  /** What the chunks were built from, to find the cells that changed since: null builds every chunk. */
  private built: { cells: ReadonlyMap<CellKey, SceneCell>; cut: ReadonlySet<CellKey>; standing: ReadonlyMap<CellKey, string> } | null = null
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
  private readonly torchModels = new TorchLayer()
  private readonly meshes = new MeshPool()
  private readonly torchDust = new TorchDust(TORCH_LIGHTS)
  private readonly tileTorches: Torch[] = []
  /** Goes up whenever `tileTorches` changes. */
  private tileTorchesVersion = 0
  private nearest: Torch[] = []
  private nearestFrom = { x: NaN, z: NaN }
  private nearestOf = -1
  /** The scene the torch dust was last placed for. */
  private dustRevision = -1
  private readonly features = new TileFeatures()
  /** Called for each hop an actor starts. */
  onHop: ((step: ActorStep) => void) | null = null
  private readonly fog = new FogLayer()
  private readonly memory = new MemoryMask()
  private readonly waters = (['water', 'murky', 'bog', 'mud', 'slime', 'acid'] as const).map((kind) => new WaterLayer(kind, this.memory))
  private readonly starwater = new StarwaterLayer(this.memory)
  private readonly lava = new LavaLayer(this.memory)
  private readonly combat = new CombatLayer()
  private readonly steps = new StepLayer()
  private readonly impacts: PendingImpact[] = []
  /** The actors' time: it stands still during a hit stop. */
  private clock = 0
  private stopUntil = 0
  private danger = 0
  /** How fast the actors walk and fight: their clock runs this many times as fast as the real one. */
  private animationSpeed = 1.5
  private readonly capMaterial = capMaterial(this.memory)
  private readonly hemisphere = new THREE.HemisphereLight()
  private readonly torchLights = Array.from({ length: TORCH_LIGHTS }, () => new THREE.PointLight())
  /** Torch lights whose shadow map no longer shows what is around them; it is drawn again when the light is on. */
  private readonly staleShadows = new Set(this.torchLights)
  /** The torch each torch light hangs at, null while it has none. */
  private readonly litTorches: (Torch | null)[] = this.torchLights.map(() => null)
  /**
   * Torch lights whose shadow map was never drawn. They are all drawn in one frame, torch or not: three makes the
   * map's depth texture again when it is first drawn, and a material bound to the old one in a frame between fails
   * every submit after it, so nothing is drawn at all.
   */
  private readonly undrawnShadows = new Set(this.torchLights)
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
  private cutAwayReach = 0
  private centreLift = 0
  private focusMemo: { scene: Scene; reach: number; extra: string; focus: Vec2[] } | null = null
  private staticKey = ''
  private sceneRevision = -1
  /** What the level's geometry was last measured against (`staticsRevision`). */
  private measured: { scene: Scene | null; cells: ReadonlyMap<CellKey, SceneCell> | null; standing: string; revision: number } = { scene: null, cells: null, standing: '', revision: 0 }
  private lastFrame = 0
  private width = 1
  private height = 1
  private dpr = 1

  constructor() {
    this.statics.add(this.features.group, this.torchModels.group)
    this.world.add(this.statics, this.actors.group, this.fog.mesh, ...this.waters.map((w) => w.mesh), this.starwater.mesh, this.lava.mesh, this.combat.group, this.steps.group, this.aimGroup, this.routeGroup, this.cursorSquare, this.hemisphere, this.sun, this.sun.target, ...this.torchLights, this.torchDust.mesh)
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
    this.renderer = new THREE.WebGPURenderer({ canvas: target, antialias: false })
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
    this.memory.update(scene)
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

  setCentreLift(px: number): void {
    this.centreLift = Math.round(px)
  }

  setCutAwayReach(cells: number): void {
    this.cutAwayReach = Math.max(0, cells)
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
    const staticKey = `${this.staticsRevision(scene)}|${this.cutAway(scene)}|${this.cam.yaw}|${this.cam.pitch}|${this.view.wallHeight}|${JSON.stringify(this.lights.walls)}|${this.lights.fog.height}|${this.lights.fog.reach}|${this.lights.water.enabled}|${this.lights.lava.enabled}`
    const changed: CellArea[] = []
    if (staticKey !== this.staticKey) {
      changed.push(...this.buildStatics(scene, this.tiles, this.materials))
      this.staticKey = staticKey
    }
    for (const door of this.features.update(dt)) changed.push({ x0: door.x, y0: door.y, x1: door.x + 1, y1: door.y + 1 })
    if (now >= this.stopUntil) this.clock += dt * this.animationSpeed
    this.landImpacts(scene, now)
    const hops = this.actors.update(this.clock, this.cam.yaw, this.upright(), (x, y) => this.features.groundAt(x, y))
    const floor = groundFloor(scene)
    for (const hop of hops) {
      this.steps.step(hop.from, hop.to, hop.seconds / this.animationSpeed, floor)
      this.onHop?.(hop)
    }
    const actorShadowsMoved = this.actors.takeShadowChange()
    if (actorShadowsMoved && this.quality.actorTorchShadows) this.markShadowsStale()
    else {
      this.markShadowsStaleAround(changed)
      if (actorShadowsMoved) this.sunStale = true
    }
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
    this.post?.setEmission(this.lava.shown || this.starwater.shown || (this.materials?.glowing === true && glowShown() && (this.glowInView() || this.actors.glowing || this.features.glowing)))
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
    for (const water of this.waters) water.dispose()
    this.starwater.dispose()
    this.lava.dispose()
    this.torchDust.dispose()
    this.memory.dispose()
    this.torchModels.dispose()
    this.post?.dispose()
    this.renderer?.dispose()
    this.renderer = null
  }

  private rebuildMaterials() {
    if (!this.tiles) return
    this.materials?.dispose()
    this.materials = new AtlasMaterials(this.tiles, this.lights.paint, this.memory)
    this.paintKey = JSON.stringify(this.lights.paint)
    this.invalidate()
  }

  private invalidate() {
    this.staticKey = ''
    if (this.scene) this.syncActors(this.scene)
  }

  private syncActors(scene: Scene) {
    if (!this.tiles || !this.materials) return
    this.actors.sync(scene, this.clock, { tiles: this.tiles, materials: this.materials, meshes: this.meshes })
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
    for (const water of this.waters) water.configure(this.lights.water)
    this.starwater.configure(this.lights.water.enabled)
    this.lava.configure(this.lights.lava)
    this.combat.configure(this.lights.combat)
    this.steps.configure(this.lights.steps)
    this.post?.configure(this.lights)
  }

  private applyQuality() {
    this.torchLights.forEach((light, i) => {
      light.castShadow = i < this.quality.torchShadows
      light.shadow.camera.layers.set(SHADOW_LAYER)
      if (this.quality.actorTorchShadows) light.shadow.camera.layers.enable(ACTOR_SHADOW_LAYER)
      if (light.castShadow) this.undrawnShadows.add(light)
    })
    this.markShadowsStale()
    if (this.renderer) this.renderer.shadowMap.enabled = this.quality.shadows
    this.post?.setAoResolution(this.quality.aoResolution)
    this.post?.setAoSamples(this.quality.aoSamples)
    this.post?.setAntialias(this.quality.antialias)
    this.applySize()
  }

  private markShadowsStale() {
    for (const light of this.torchLights) this.staleShadows.add(light)
    this.sunStale = true
  }

  /** The shadow maps that see a part of these areas no longer show what is there: a torch's within its reach, the sun's within its own. */
  private markShadowsStaleAround(areas: readonly CellArea[]) {
    if (!areas.length) return
    const near = (x: number, z: number, reach: number) => areas.some((a) => x > a.x0 - reach && x < a.x1 + reach && z > a.y0 - reach && z < a.y1 + reach)
    for (const light of this.torchLights) if (near(light.position.x, light.position.z, this.lights.torch.distance)) this.staleShadows.add(light)
    const sunAt = this.sun.target.position
    if (near(sunAt.x, sunAt.z, SUN_REACH * Math.SQRT2)) this.sunStale = true
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
    if (this.centreLift) camera.setViewOffset(this.width, this.height, 0, this.centreLift, this.width, this.height)
    else camera.clearViewOffset()
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
    const fromX = this.cam.eyeX + 0.5
    const fromZ = this.cam.eyeY + 0.5
    const fade = (torch: Torch) => 1 - THREE.MathUtils.smoothstep(Math.hypot(torch.position.x - fromX, torch.position.z - fromZ), TORCH_FADE_START, TORCH_FADE_END)
    const nearest = this.nearestTorches()
    const shadows = this.torchLights.filter((light) => light.castShadow).length
    const wanted = assignTorches(this.litTorches.slice(0, shadows), nearest.slice(0, shadows))
    const due = wanted
      .map((torch, i) => ({ torch, i, moves: !sameTorch(torch, this.litTorches[i]) }))
      .filter(({ torch, i, moves }) => torch !== null && fade(torch) > 0 && (moves || this.staleShadows.has(this.torchLights[i])))
      .sort((a, b) => Number(b.moves) - Number(a.moves) || a.i - b.i)[0]
    const shadowed = wanted.map((torch, i) => (i === due?.i || sameTorch(torch, this.litTorches[i]) ? torch : this.litTorches[i]))
    const unshadowed = assignTorches(this.litTorches.slice(shadows), nearest.filter((torch) => !shadowed.some((held) => sameTorch(held, torch))))
    const placed = [...shadowed, ...unshadowed]
    let moved = this.sceneRevision !== this.dustRevision
    this.torchLights.forEach((light, i) => {
      const torch = placed[i]
      moved ||= torch !== this.litTorches[i]
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
    for (const light of this.undrawnShadows) if (light.castShadow) light.shadow.needsUpdate = true
    this.undrawnShadows.clear()
    if (!moved) return
    this.dustRevision = this.sceneRevision
    this.torchDust.update(this.litTorches.map((torch) => (torch && this.scene && getCell(this.scene, torch.cell.x, torch.cell.y)?.visibility === 'visible' ? torch : null)))
  }

  /** The torches nearest to the cell the camera looks at, nearest first, as many as there are torch lights: sorted again only when that cell or the torches change. */
  private nearestTorches(): Torch[] {
    const x = Math.round(this.cam.eyeX) + 0.5
    const z = Math.round(this.cam.eyeY) + 0.5
    if (x !== this.nearestFrom.x || z !== this.nearestFrom.z || this.nearestOf !== this.tileTorchesVersion) {
      this.nearestFrom = { x, z }
      this.nearestOf = this.tileTorchesVersion
      const away = (torch: Torch) => (torch.position.x - x) ** 2 + (torch.position.z - z) ** 2
      this.nearest = [...this.tileTorches].sort((a, b) => away(a) - away(b)).slice(0, TORCH_LIGHTS)
    }
    return this.nearest
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
    const extra = target !== undefined && isPointable(getCell(scene, target.x, target.y)) ? target : null
    const extraKey = extra ? `${extra.x},${extra.y}` : ''
    const memo = this.focusMemo
    if (memo && memo.scene === scene && memo.reach === this.cutAwayReach && memo.extra === extraKey) return memo.focus
    const focus = cutAwayFocus(scene, extra, this.cutAwayReach)
    this.focusMemo = { scene, reach: this.cutAwayReach, extra: extraKey, focus }
    return focus
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
    for (const chunk of [...this.chunks.values(), ...this.spriteChunks.values()]) this.dropChunk(chunk)
    this.chunks.clear()
    this.spriteChunks.clear()
    this.chunksKey = ''
    this.built = null
    this.tileTorches.length = 0
    this.tileTorchesVersion++
  }

  private dropChunk(chunk: StaticChunk) {
    this.meshes.give(chunk.group)
    for (const g of chunk.geometries) g.dispose()
  }

  private addTileTorch(chunk: StaticChunk, x: number, y: number, normal: { x: number; y: number }, wallHeight: number) {
    if (!this.assets) return
    const height = TORCH_HEIGHT * Math.min(1, wallHeight / 0.85)
    const model = new THREE.Matrix4().compose(new THREE.Vector3(x + 0.5 + normal.x * 0.5, height, y + 0.5 + normal.y * 0.5), new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), Math.atan2(normal.x, normal.y)), new THREE.Vector3(KIT_SCALE, KIT_SCALE, KIT_SCALE))
    chunk.placements.push({ model, glow: new THREE.Vector3(x + 0.5 + normal.x * 0.62, height + 0.2, y + 0.5 + normal.y * 0.62) })
    chunk.torches.push({ position: new THREE.Vector3(x + 0.5 + normal.x * 0.75, height + 0.2, y + 0.5 + normal.y * 0.75), phase: (x * 7 + y * 13) % 10, cell: { x: x + normal.x, y: y + normal.y } })
  }

  /** The level's geometry in chunks of `CHUNK_CELLS` square: only a chunk whose cells, cut-away walls or items changed is built again. Returns where the geometry changed. */
  private buildStatics(scene: Scene, tiles: TileSource, materials: AtlasMaterials): CellArea[] {
    const changed: CellArea[] = []
    const { stretch, lean } = this.upright()
    const chunksKey = `${this.view.wallHeight}|${JSON.stringify(this.lights.walls)}|${this.lights.water.enabled}|${this.lights.lava.enabled}|${this.assets !== null}`
    if (chunksKey !== this.chunksKey || materials !== this.chunkMaterials) {
      this.clearStatics()
      this.chunksKey = chunksKey
      this.chunkMaterials = materials
      changed.push(EVERYWHERE)
    }
    const spritesView = `${this.cam.yaw}|${stretch}|${lean}`
    const turned = spritesView !== this.spritesView
    this.spritesView = spritesView
    const focus = this.focus(scene)
    const cut = cutAwayCells(scene, focus, this.cam.yaw, -this.cam.pitch, this.view)
    const standing = standingByCell(scene)
    const dirty = this.built && dirtyChunks(this.built, scene.cells, cut, standing)
    this.built = { cells: scene.cells, cut, standing }
    const plan = planScene(scene, this.cam.yaw, -this.cam.pitch, this.view, focus, dirty ? (x, y) => dirty.has(chunkId(x, y)) : undefined)
    const spritePlan = turned && dirty ? planScene(scene, this.cam.yaw, -this.cam.pitch, this.view, focus, undefined, true) : plan
    const revision = this.staticsRevision(scene)
    this.fog.build(scene, revision, this.lights.fog.height, this.lights.fog.reach)
    for (const water of this.waters) water.build(scene, revision)
    this.starwater.build(scene, revision)
    this.lava.build(scene, revision)
    if (this.features.build(scene, tiles, materials, this.meshes, this.view.wallHeight, groundBasis(this.cam.yaw).forward)) changed.push(EVERYWHERE)
    const grounds = new Map<string, ChunkPart>()
    const sprites = new Map<string, ChunkPart>()
    const partAt = (parts: Map<string, ChunkPart>, x: number, y: number) => {
      const id = chunkId(x, y)
      let part = parts.get(id)
      if (!part) parts.set(id, (part = { ground: [], walls: [], sprites: [] }))
      return part
    }
    for (const g of plan.ground) if (!isTileFeature(getCell(scene, g.x, g.y))) partAt(grounds, g.x, g.y).ground.push(g)
    for (const w of plan.walls) if (!isTileFeature(getCell(scene, w.x, w.y))) partAt(grounds, w.x, w.y).walls.push(w)
    for (const sprite of spritePlan.sprites) {
      const b = sprite.billboard
      if (b && actorId(b) !== undefined) continue
      if (!b && isTileFeature(getCell(scene, sprite.x, sprite.y))) continue
      partAt(sprites, sprite.x, sprite.y).sprites.push(sprite)
    }
    const update = (chunks: Map<string, StaticChunk>, parts: Map<string, ChunkPart>, keyOf: (part: ChunkPart) => string, everywhere: boolean) => {
      for (const [id, part] of parts) {
        const key = keyOf(part)
        const old = chunks.get(id)
        if (old?.key === key) continue
        if (old) this.dropChunk(old)
        const chunk = this.buildChunk(scene, tiles, materials, part, key)
        chunks.set(id, chunk)
        this.statics.add(chunk.group)
        changed.push(chunkArea(id))
      }
      for (const [id, chunk] of chunks)
        if (!parts.has(id) && (everywhere || !dirty || dirty.has(id))) {
          this.dropChunk(chunk)
          chunks.delete(id)
          changed.push(chunkArea(id))
        }
    }
    update(this.chunks, grounds, (part) => JSON.stringify([part.ground.map((g) => [g.x, g.y, this.groundTile(scene, g), g.lift]), part.walls.map((w) => [w.x, w.y, w.height, w.top, w.faces])]), !dirty)
    update(this.spriteChunks, sprites, (part) => `${spritesView}|${JSON.stringify(part.sprites.map((sprite) => [sprite.x, sprite.y, sprite.lying, sprite.pile, sprite.layers]))}`, !dirty || turned)
    this.tileTorches.length = 0
    for (const chunk of this.chunks.values()) this.tileTorches.push(...chunk.torches)
    this.tileTorchesVersion++
    if (changed.length && this.assets) this.torchModels.set(this.assets.torch, [...this.chunks.values()].flatMap((chunk) => chunk.placements))
    return changed
  }

  /** `cutAwayKey` for the view now, worked out again only when the cells in focus, the view or the level's cells change. */
  private cutAway(scene: Scene): string {
    const focus = this.focus(scene)
    const inputs = `${focus.map((f) => `${f.x},${f.y}`).join(';')}|${this.cam.yaw}|${this.cam.pitch}|${this.view.wallHeight}|${this.staticsRevision(scene)}`
    if (inputs !== this.cutAwayInputs) {
      this.cutAwayInputs = inputs
      this.cutAwayCache = cutAwayKey(scene, focus, this.cam.yaw, -this.cam.pitch, this.view)
    }
    return this.cutAwayCache
  }

  /** Whether a chunk with a glowing tile is near enough to the middle of the view to be on the screen. */
  private glowInView(): boolean {
    const halfWidth = this.width / 2 / this.view.zoom
    const halfHeight = this.height / 2 / this.view.zoom / Math.max(0.2, Math.sin(-this.cam.pitch))
    const reach = Math.hypot(halfWidth, halfHeight) + 1
    const x = this.cam.eyeX + 0.5
    const y = this.cam.eyeY + 0.5
    for (const [id, chunk] of [...this.chunks, ...this.spriteChunks]) {
      if (!chunk.glows) continue
      const area = chunkArea(id)
      if (Math.hypot(Math.max(area.x0 - x, 0, x - area.x1), Math.max(area.y0 - y, 0, y - area.y1)) < reach) return true
    }
    return false
  }

  /** The floor tile a ground layer shows: under drawn water, the nearest land's floor. */
  private groundTile(scene: Scene, g: GroundLayer): TileId {
    const liquid = getCell(scene, g.x, g.y)?.liquid
    const drawn = liquid !== undefined && !FILMS.has(liquid.kind) && (liquid.kind === 'lava' ? this.lights.lava.enabled : this.lights.water.enabled)
    return g.lift === 0 && drawn ? (shoreFloor(scene, g.x, g.y) ?? g.tile) : g.tile
  }

  private buildChunk(scene: Scene, tiles: TileSource, materials: AtlasMaterials, part: ChunkPart, key: string): StaticChunk {
    const chunk: StaticChunk = { key, group: new THREE.Group(), geometries: [], torches: [], placements: [], glows: false }
    const groundBatches = new Map<string, QuadBatch>()
    const spriteBatches = new Map<string, QuadBatch>()
    let batches = groundBatches
    const quad = (tile: TileId, corners: [Point3, Point3, Point3, Point3], texelHeight: number | undefined, shade: number) => {
      const rect = tiles.tile(tile)
      if (!rect) return
      const placed = materials.forTile(rect)
      if (!placed) return
      if (placed.label && placed.label.glow > 0) chunk.glows = true
      let batch = batches.get(placed.rect.atlas)
      if (!batch) batches.set(placed.rect.atlas, (batch = new QuadBatch(placed.width, placed.height, batches === spriteBatches ? SPRITE_LEAN : 0, ATLAS_INSET)))
      batch.add(corners, placed.rect.sx, placed.rect.sy, rect.w, texelHeight ?? rect.h, shade)
    }

    const caps = new QuadBatch(1, 1)
    const moss = new THREE.Color(this.lights.walls.mossColour)
    const mossAmount = this.lights.walls.moss
    for (const g of part.ground) {
      const y = g.lift * LIFT_STEP
      quad(this.groundTile(scene, g), [[g.x, y, g.y], [g.x + 1, y, g.y], [g.x + 1, y, g.y + 1], [g.x, y, g.y + 1]], undefined, 1)
    }
    for (const w of part.walls) {
      const h = w.height
      const topRect = tiles.tile(w.top)
      const [r, g, b] = topRect ? materials.averageColour(topRect) : [0.3, 0.3, 0.3]
      const capColour: Point3 = [THREE.MathUtils.lerp(r * CAP_SHADE, moss.r, mossAmount), THREE.MathUtils.lerp(g * CAP_SHADE, moss.g, mossAmount), THREE.MathUtils.lerp(b * CAP_SHADE, moss.b, mossAmount)]
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
        quad(face.tile, [[ax, faceTop, ay], [bx, faceTop, by], [bx, 0, by], [ax, 0, ay]], texelHeight, 1)
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
      quad(layer.tile, [[l, y, y0], [r, y, y0], [r, y, y1], [l, y, y1]], f.texelHeight, copy ? PILE_SHADE : 1)
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
      quad(layer.tile, [at(l, top), at(r, top), at(r, bottom), at(l, bottom)], f.texelHeight, copy ? PILE_SHADE : 1)
    }
    for (const s of part.sprites) {
      const copies = s.pile ? PILE_COPIES : []
      for (const copy of copies) s.layers.forEach((layer, i) => (s.lying ? lyingLayer(s, layer, i, copy) : standingLayer(s, layer, copy)))
      s.layers.forEach((layer, i) => (s.lying ? lyingLayer(s, layer, i, null) : standingLayer(s, layer, null)))
    }

    if (caps.positions.length) {
      const geometry = caps.geometry()
      chunk.geometries.push(geometry)
      const mesh = this.meshes.take(geometry, this.capMaterial)
      mesh.receiveShadow = true
      chunk.group.add(mesh)
    }
    for (const [group, sprites] of [[groundBatches, false], [spriteBatches, true]] as const) {
      for (const [atlas, batch] of group) {
        const entry = materials.material(atlas)
        if (!entry) continue
        const geometry = batch.geometry()
        chunk.geometries.push(geometry)
        const mesh = this.meshes.take(geometry, sprites ? entry.sprite : entry.material)
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

function chunkId(x: number, y: number): string {
  return `${Math.floor(x / CHUNK_CELLS)},${Math.floor(y / CHUNK_CELLS)}`
}

/**
 * The chunks that may build differently since `built`: around every cell whose drawn content changed (as far as a
 * water cell looks for the floor of its shore, which covers the wall faces of the neighbours), and the cells whose
 * cut-away or items changed.
 */
function dirtyChunks(built: { cells: ReadonlyMap<CellKey, SceneCell>; cut: ReadonlySet<CellKey>; standing: ReadonlyMap<CellKey, string> }, cells: ReadonlyMap<CellKey, SceneCell>, cut: ReadonlySet<CellKey>, standing: ReadonlyMap<CellKey, string>): Set<string> {
  const dirty = new Set<string>()
  const around = (x: number, y: number, reach: number) => {
    for (const dy of [-reach, reach]) for (const dx of [-reach, reach]) dirty.add(chunkId(x + dx, y + dy))
  }
  const at = (key: CellKey, reach: number) => {
    const { x, y } = keyToXY(key)
    around(x, y, reach)
  }
  if (built.cells !== cells) {
    for (const [key, cell] of cells) {
      const old = built.cells.get(key)
      if (old !== cell && (!old || drawnCell(old) !== drawnCell(cell))) at(key, SHORE_SEARCH)
    }
    for (const key of built.cells.keys()) if (!cells.has(key)) at(key, SHORE_SEARCH)
  }
  for (const key of cut) if (!built.cut.has(key)) at(key, 0)
  for (const key of built.cut) if (!cut.has(key)) at(key, 0)
  for (const [key, signature] of standing) if (built.standing.get(key) !== signature) at(key, 0)
  for (const key of built.standing.keys()) if (!standing.has(key)) at(key, 0)
  return dirty
}

function chunkArea(id: string): CellArea {
  const [cx, cy] = id.split(',').map(Number)
  return { x0: cx * CHUNK_CELLS, y0: cy * CHUNK_CELLS, x1: (cx + 1) * CHUNK_CELLS, y1: (cy + 1) * CHUNK_CELLS }
}

function applyPointLight(light: THREE.PointLight, spec: PointLightSpec) {
  light.color.set(spec.colour)
  light.intensity = spec.intensity
  light.distance = spec.distance
  light.decay = spec.decay
  light.shadow.camera.far = spec.distance
}

/** The wall tops: a faint stone grain under the vertex colours, darker on a remembered cell. */
function capMaterial(memory: MemoryMask) {
  const material = new ClayMaterial({ vertexColors: true })
  material.colorNode = remembered(texture(capTexture()), memory)
  return material
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
        if (cell && !cell.occluder && onLand(cell)) return cell.floorTile
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

function onLand(cell: SceneCell) {
  return !cell.liquid || FILMS.has(cell.liquid.kind)
}

/** Whether blood, dust and footprints show on a cell: open dry ground in sight. */
function groundFloor(scene: Scene) {
  return (x: number, y: number) => {
    const cell = getCell(scene, x, y)
    return cell !== undefined && !cell.occluder && onLand(cell) && cell.visibility === 'visible'
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

/** A cell as the geometry sees it: whether it is in sight or only remembered is the memory mask's, not the geometry's. */
function drawnCell(cell: SceneCell): string {
  let drawn = drawnCells.get(cell)
  if (drawn === undefined) {
    drawn = JSON.stringify(cell, (field, value) => (UNDRAWN_CELL_FIELDS.has(field) ? undefined : field === 'visibility' ? value === 'unseen' : value))
    drawnCells.set(cell, drawn)
  }
  return drawn
}

/** `drawnCell` of each cell object: the scene builder hands over the same object for a cell it did not touch. */
const drawnCells = new WeakMap<SceneCell, string>()

/** What stands on each cell in the level's geometry: the items and features shown that are not actors. */
function standingByCell(scene: Scene): Map<CellKey, string> {
  const standing = new Map<CellKey, string>()
  for (const b of scene.billboards) {
    if (actorId(b) !== undefined || !isShown(b, scene)) continue
    const key = cellKey(b.x, b.y)
    standing.set(key, `${standing.get(key) ?? ''}|${b.kind}:${b.tile},${b.lying ? 1 : 0},${hasItemsUnder(b) ? 1 : 0},${b.layers?.map((l) => `${l.tile}/${l.ox ?? 0}/${l.oy ?? 0}/${l.ymax ?? ''}`).join(';') ?? ''}`)
  }
  return standing
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

