import * as THREE from 'three/webgpu'
import type { Billboard, Scene, TileId, TileSource } from '@isorun/scene'
import type { AtlasMaterials } from './materials.js'
import type { MeshPool } from './mesh-pool.js'
import { groundBasis, isShown, tileFrame } from './plan.js'
import { ATLAS_INSET, QuadBatch, SPRITE_LEAN, type Point3 } from './quads.js'
import { ACTOR_MOTIONS, ACTOR_SHADOW_LAYER, NO_EFFECTS, setActorLook, castShadow as setCastShadow, type ActorLook } from './shading.js'

export type ActorId = string | number

export type ActorAction = 'attack' | 'hit' | 'miss' | 'death'

/** Seconds of one hop from a cell to the next. */
export const STEP_SECONDS = 0.2
const HOP_HEIGHT = 0.14
const LUNGE_SECONDS = 0.36
const LUNGE_REACH = 0.4
const WINDUP_REACH = 0.12
const WINDUP_END = 0.3
/** The part of the lunge where it reaches the target: hits and kills land then (`IMPACT_DELAY`). */
const STRIKE_END = 0.45
/** Seconds from the start of an attack to the moment it reaches the target. */
export const IMPACT_DELAY = LUNGE_SECONDS * STRIKE_END
/** Seconds from the start of an attack to the end of its pull back, when the blow swings forward. */
export const STRIKE_DELAY = LUNGE_SECONDS * WINDUP_END
const HIT_SECONDS = 0.3
const KNOCKBACK = 0.18
const DODGE_SECONDS = 0.3
const DODGE_DISTANCE = 0.2
const DODGE_TILT = 0.2
const SQUASH = 0.25
const SHAKE_PER_SECOND = 40
const DEATH_SECONDS = 0.9
const DEATH_FLASH_SECONDS = 0.12
const DEATH_FALL_SECONDS = 0.35
const DEATH_KNOCKBACK = 0.3
const DEATH_FALL_ANGLE = Math.PI / 2.2
/** A move of more cells than this in one scene is a jump (a blink, a level's stairs), not a walk to play hop by hop. */
const MAX_WALK = 8
/** How much faster each hop plays for every hop still waiting behind it, so a fast travel keeps up. */
const CATCH_UP = 0.5
/** The most a hop may start in the past (a frame late): a step that came after the hop before it landed starts where it is, not partly done. */
const HOP_LATE = 1 / 60
/** How much of the way to the latest time between steps the walk's pace goes with each step, so the hops fill the time between the server's steps. */
const PACE_EASE = 0.3
/** The pace a walk may take, as parts of `STEP_SECONDS`: steps further apart than the slowest are a new walk. */
const FASTEST_PACE = 0.6
const SLOWEST_PACE = 1.5
/** How much of the way to the speed the waiting hops want each new hop goes, so a walk speeds up and slows down smoothly. */
const HOP_EASE = 0.35
const HURT_TINT = { r: 4, g: 0.6, b: 0.5 }
const FLASH_TINT = { r: 4, g: 4, b: 4 }

/** The identity of a billboard that moves and acts: the player, or a monster with crawl's client id on its `ref`. */
export function actorId(b: Billboard): ActorId | undefined {
  if (b.kind === 'player') return 'player'
  if (b.kind !== 'monster') return undefined
  const ref: unknown = b.ref
  if (typeof ref === 'object' && ref !== null && 'id' in ref && typeof ref.id === 'number') return ref.id
  return undefined
}

/** How upright sprites stand: `stretch` up, and `lean` back from the camera in radians. */
export interface UprightPose {
  stretch: number
  lean: number
}

export interface ActorStep {
  id: ActorId
  from: { x: number; y: number }
  to: { x: number; y: number }
  /** How long the hop takes, on the actors' clock. */
  seconds: number
}

export interface ActorEnvironment {
  tiles: TileSource
  materials: AtlasMaterials
  meshes: MeshPool
}

/** How far each layer of a sprite stands in front of the one under it, in cells, so the layers never fight over the same depth. */
const LAYER_LIFT = 0.0015
/** How far a status icon stands in front of its sprite, in cells. */
const ICON_LIFT = 0.03

function spriteKey(b: Billboard) {
  return `${b.tile}|${(b.layers ?? []).map((l) => `${l.tile},${l.ox ?? 0},${l.oy ?? 0},${l.ymax ?? ''}`).join(';')}|${(b.statusIcons ?? []).map((i) => `${i.tile},${i.ox},${i.oy},${i.at ?? ''}`).join(';')}`
}

/**
 * What the sprite is drawn from, bottom to top: its layers, then its status icons (the damage bar,
 * a poison drop, a net), as WebTiles draws them on the tile. An icon pinned to the top stands on the
 * frame's top edge; the marks about the square (an item under the monster) are left to the floor.
 */
function spriteLayers(b: Billboard, tiles: TileSource): { tile: TileId; ox: number; oy: number; ymax: number | undefined; icon: boolean }[] {
  const layers = b.layers?.map((l) => ({ tile: l.tile, ox: l.ox ?? 0, oy: l.oy ?? 0, ymax: l.ymax, icon: false })) ?? [{ tile: b.tile, ox: 0, oy: 0, ymax: undefined, icon: false }]
  for (const i of b.statusIcons ?? []) {
    if (i.square) continue
    const rect = tiles.tile(i.tile)
    if (!rect) continue
    layers.push({ tile: i.tile, ox: i.ox, oy: i.at === 'top' ? -rect.oy : i.oy, ymax: undefined, icon: true })
  }
  return layers
}

class SpriteVisual {
  readonly object = new THREE.Group()
  private readonly geometries: THREE.BufferGeometry[] = []
  private readonly look: ActorLook = { tint: new THREE.Color(1, 1, 1), opacity: 1, motion: ACTOR_MOTIONS.none, phase: Math.random() * Math.PI * 2, effects: { ...NO_EFFECTS } }

  readonly atlas: AtlasMaterials
  private readonly meshes: MeshPool

  constructor(
    b: Billboard,
    env: ActorEnvironment,
    readonly key: string,
    readonly castShadow: boolean,
  ) {
    this.atlas = env.materials
    this.meshes = env.meshes
    for (const effect of b.effects ?? []) this.look.effects[effect] = 1
    const layers = spriteLayers(b, env.tiles)
    const batches = new Map<string, { batch: QuadBatch; material: THREE.Material }>()
    for (const [index, layer] of layers.entries()) {
      const rect = env.tiles.tile(layer.tile)
      if (!rect) continue
      const placed = env.materials.forTile(rect)
      if (!placed) continue
      if (index === 0 && placed.label) this.look.motion = ACTOR_MOTIONS[placed.label.motion]
      let entry = batches.get(placed.rect.atlas)
      if (!entry) batches.set(placed.rect.atlas, (entry = { batch: new QuadBatch(placed.width, placed.height, SPRITE_LEAN, ATLAS_INSET), material: placed.actor }))
      const f = tileFrame(rect, layer.ox, layer.oy, layer.ymax)
      if (f.height <= 0) continue
      const left = f.left - 0.5
      const right = left + f.width
      const top = 1 - f.top
      const bottom = 1 - f.top - f.height
      const z = layer.icon ? ICON_LIFT : index * LAYER_LIFT
      const corners: [Point3, Point3, Point3, Point3] = [[left, top, z], [right, top, z], [right, bottom, z], [left, bottom, z]]
      entry.batch.add(corners, placed.rect.sx, placed.rect.sy, rect.w, f.texelHeight, 1)
    }
    for (const { batch, material } of batches.values()) {
      const geometry = batch.geometry()
      this.geometries.push(geometry)
      const mesh = env.meshes.take(geometry, material)
      setActorLook(mesh, this.look)
      setCastShadow(mesh, castShadow, ACTOR_SHADOW_LAYER)
      this.object.add(mesh)
    }
  }

  setFlash(amount: number, tint: { r: number; g: number; b: number }) {
    this.look.tint.setRGB(1 + amount * tint.r, 1 + amount * (tint.g - 1), 1 + amount * (tint.b - 1))
  }

  setOpacity(opacity: number) {
    this.look.opacity = opacity
  }

  dispose() {
    this.meshes.give(this.object)
    for (const g of this.geometries) g.dispose()
  }
}

class Actor {
  readonly root = new THREE.Group()
  private readonly holder = new THREE.Group()
  x: number
  y: number
  private hopFrom: { x: number; y: number }
  private hopTo: { x: number; y: number }
  private hopStart = -Infinity
  private hopSeconds = STEP_SECONDS
  /** Seconds between the last steps that came, eased: how long a hop of the walk under way takes. */
  private pace = STEP_SECONDS
  private lastArrival = -Infinity
  /** The cells still to hop to after `hopTo`, in order: a walk of several cells that came in one scene. */
  private readonly path: { x: number; y: number }[] = []
  /** The hops that started since the last `takeHops`. */
  private readonly started: { from: { x: number; y: number }; to: { x: number; y: number }; seconds: number }[] = []
  private lunge: { start: number; dx: number; dy: number } | null = null
  private hit = { start: -Infinity, dx: 0, dy: 0 }
  private dodge = { start: -Infinity, dx: 0, dy: 0 }
  private fall = { dx: 0, dy: 0 }
  deathStart: number | null = null
  /** Whether the last update moved the actor more than its breath: a hop, an attack, a hit or a death. */
  moving = false
  private readonly phase = Math.random() * Math.PI * 2

  constructor(
    readonly id: ActorId,
    public visual: SpriteVisual,
    b: Billboard,
  ) {
    this.x = b.x
    this.y = b.y
    this.hopFrom = { x: b.x, y: b.y }
    this.hopTo = { x: b.x, y: b.y }
    this.root.add(this.holder)
    this.holder.add(visual.object)
  }

  setVisual(visual: SpriteVisual) {
    this.holder.remove(this.visual.object)
    this.visual.dispose()
    this.visual = visual
    this.holder.add(visual.object)
  }

  /**
   * The actor is on cell `x`, `y` now. A walk of a few cells hops through the cells between, one hop after
   * another; a longer move is a jump, shown at once.
   */
  moveTo(x: number, y: number, now: number) {
    if (x === this.x && y === this.y) return
    if (Math.max(Math.abs(x - this.x), Math.abs(y - this.y)) > MAX_WALK) {
      this.path.length = 0
      this.hopFrom = { x, y }
      this.hopTo = { x, y }
      this.hopStart = -Infinity
    } else {
      const cells = Math.max(Math.abs(x - this.x), Math.abs(y - this.y))
      this.keepPace((now - this.lastArrival) / cells)
      this.lastArrival = now
      let at = { x: this.x, y: this.y }
      while (at.x !== x || at.y !== y) {
        at = { x: at.x + Math.sign(x - at.x), y: at.y + Math.sign(y - at.y) }
        this.path.push(at)
      }
    }
    this.x = x
    this.y = y
    this.advance(now)
  }

  private keepPace(secondsPerCell: number) {
    if (secondsPerCell > STEP_SECONDS * SLOWEST_PACE) this.pace = STEP_SECONDS
    else this.pace += (Math.max(STEP_SECONDS * FASTEST_PACE, secondsPerCell) - this.pace) * PACE_EASE
  }

  /** The hops that started since the last call. */
  takeHops() {
    return this.started.splice(0)
  }

  /** Start the next hop of the path once the one under way has landed. */
  private advance(now: number) {
    while (this.path.length && now >= this.hopStart + this.hopSeconds) {
      const end = this.hopStart + this.hopSeconds
      const next = this.path.shift()
      if (!next) break
      const continuing = now - end < this.hopSeconds
      this.hopStart = Math.max(end, now - HOP_LATE)
      const wanted = this.pace / (1 + this.path.length * CATCH_UP)
      this.hopSeconds = continuing ? this.hopSeconds + (wanted - this.hopSeconds) * HOP_EASE : wanted
      this.hopFrom = this.hopTo
      this.hopTo = next
      this.started.push({ from: this.hopFrom, to: next, seconds: this.hopSeconds })
    }
  }

  /** `target`: whom an attack goes to, or who made a hit or a kill. */
  act(action: ActorAction, now: number, target?: { x: number; y: number }) {
    if (this.deathStart !== null) return
    const len = target ? Math.hypot(target.x - this.x, target.y - this.y) || 1 : 1
    const dx = target ? (target.x - this.x) / len : 0
    const dy = target ? (target.y - this.y) / len : 0
    if (action === 'attack' && target) this.lunge = { start: now, dx, dy }
    if (action === 'hit') this.hit = { start: now, dx: -dx, dy: -dy }
    if (action === 'miss') this.dodge = { start: now, dx: -dy, dy: dx }
    if (action === 'death') {
      this.deathStart = now
      this.fall = { dx: -dx, dy: -dy }
    }
  }

  get cell() {
    return { x: this.x, y: this.y }
  }

  /** When the attack under way reaches its target, or undefined with no attack under way. */
  impactTime(): number | undefined {
    return this.lunge ? this.lunge.start + IMPACT_DELAY : undefined
  }

  get finished() {
    return this.deathStart !== null && this.lastNow - this.deathStart > DEATH_SECONDS
  }

  private lastNow = 0

  update(now: number, yaw: number, { stretch, lean }: UprightPose, groundAt: (x: number, y: number) => number) {
    this.lastNow = now
    this.advance(now)
    const moveT = Math.min(1, (now - this.hopStart) / this.hopSeconds)
    const cell = this.shownCell(now)
    let px = cell.x + 0.5
    let pz = cell.y + 0.5
    const lift = moveT < 1 ? Math.sin(Math.PI * moveT) * HOP_HEIGHT * Math.min(1, this.hopSeconds / STEP_SECONDS) : 0
    this.moving = moveT < 1 || this.path.length > 0 || this.lunge !== null || this.deathStart !== null
    if (this.lunge) {
      const t = (now - this.lunge.start) / LUNGE_SECONDS
      if (t >= 1) this.lunge = null
      else {
        const reach = lungeReach(t)
        px += this.lunge.dx * reach
        pz += this.lunge.dy * reach
      }
    }
    const hitT = (now - this.hit.start) / HIT_SECONDS
    const hitting = hitT >= 0 && hitT < 1
    const recoil = hitting ? (1 - hitT) * (1 - hitT) : 0
    this.moving ||= hitting
    if (hitting) {
      const push = Math.sin(Math.PI * Math.min(1, hitT * 2)) * KNOCKBACK * (1 - hitT)
      px += this.hit.dx * push + Math.sin(hitT * SHAKE_PER_SECOND) * 0.04 * (1 - hitT)
      pz += this.hit.dy * push
    }
    const dodgeT = (now - this.dodge.start) / DODGE_SECONDS
    const dodging = dodgeT >= 0 && dodgeT < 1
    this.moving ||= dodging
    const sway = dodging ? Math.sin(Math.PI * dodgeT) : 0
    px += this.dodge.dx * sway * DODGE_DISTANCE
    pz += this.dodge.dy * sway * DODGE_DISTANCE
    let flash = hitting ? 1 - hitT : 0
    const { right: across } = groundBasis(yaw)
    let tilt = sway * DODGE_TILT * (this.dodge.dx * across.x + this.dodge.dy * across.y < 0 ? 1 : -1)
    let opacity = 1
    let sink = 1
    if (this.deathStart !== null) {
      const age = Math.max(0, now - this.deathStart)
      flash = Math.max(flash, age < DEATH_FLASH_SECONDS ? 1 : 0)
      const fallT = Math.min(1, age / DEATH_FALL_SECONDS)
      const eased = 1 - (1 - fallT) * (1 - fallT)
      px += this.fall.dx * DEATH_KNOCKBACK * eased
      pz += this.fall.dy * DEATH_KNOCKBACK * eased
      const { right } = groundBasis(yaw)
      const side = this.fall.dx * right.x + this.fall.dy * right.y < 0 ? 1 : -1
      tilt = side * DEATH_FALL_ANGLE * fallT * fallT
      const fadeT = Math.max(0, (age - DEATH_FALL_SECONDS) / (DEATH_SECONDS - DEATH_FALL_SECONDS))
      opacity = 1 - fadeT
      sink = 1 - fadeT * 0.5
    }
    this.visual.setFlash(flash, this.id === 'player' ? HURT_TINT : FLASH_TINT)
    this.visual.setOpacity(opacity)
    const ground = groundAt(this.hopFrom.x, this.hopFrom.y) + (groundAt(this.hopTo.x, this.hopTo.y) - groundAt(this.hopFrom.x, this.hopFrom.y)) * Math.min(1, Math.max(0, moveT))
    this.root.position.set(px, lift + ground, pz)

    this.holder.rotation.set(-lean, -yaw, tilt, 'YXZ')
    const breath = Math.sin(now * 2.6 + this.phase) * 0.025
    const scaleY = stretch * (1 + breath) * (1 - recoil * SQUASH) * sink
    this.holder.scale.set((1 - breath * 0.5) * (1 + recoil * SQUASH), scaleY, 1)
  }

  shown() {
    return this.shownCell(this.lastNow)
  }

  private shownCell(now: number) {
    const t = Math.min(1, Math.max(0, (now - this.hopStart) / this.hopSeconds))
    const eased = t * t * (3 - 2 * t)
    return { x: this.hopFrom.x + (this.hopTo.x - this.hopFrom.x) * eased, y: this.hopFrom.y + (this.hopTo.y - this.hopFrom.y) * eased }
  }

  dispose() {
    this.visual.dispose()
  }
}

/** The things that move and act, kept from one scene to the next so they can glide and play actions. */
export class ActorLayer {
  readonly group = new THREE.Group()
  private readonly actors = new Map<ActorId, Actor>()
  private shadowChange = true

  sync(scene: Scene, now: number, env: ActorEnvironment) {
    const seen = new Set<ActorId>()
    for (const b of scene.billboards) {
      const id = actorId(b)
      if (id === undefined || !isShown(b, scene)) continue
      seen.add(id)
      const key = spriteKey(b)
      let actor = this.actors.get(id)
      if (!actor) {
        actor = new Actor(id, new SpriteVisual(b, env, key, id !== 'player'), b)
        this.actors.set(id, actor)
        this.group.add(actor.root)
        this.shadowChange = true
      } else if (actor.visual.key !== key || actor.visual.atlas !== env.materials) {
        actor.setVisual(new SpriteVisual(b, env, key, id !== 'player'))
        this.shadowChange = true
      }
      actor.moveTo(b.x, b.y, now)
    }
    for (const [id, actor] of this.actors) if (!seen.has(id) && actor.deathStart === null) this.remove(id, actor)
  }

  act(id: ActorId, action: ActorAction, now: number, target?: { x: number; y: number }) {
    this.actors.get(id)?.act(action, now, target)
  }

  /** When a blow from the actor on `cell` lands: when its attack under way reaches the target, or a whole attack from `now`. */
  impactFrom(cell: { x: number; y: number } | undefined, now: number): number {
    const attacker = cell && [...this.actors.values()].find((a) => a.x === cell.x && a.y === cell.y && a.deathStart === null)
    const time = attacker?.impactTime()
    return time === undefined ? now + IMPACT_DELAY : Math.max(now, time)
  }

  /** Where the actor is drawn now, in cells: between two cells during a hop. */
  shownCellOf(id: ActorId): { x: number; y: number } | undefined {
    return this.actors.get(id)?.shown()
  }

  /** The cell the actor stands on, also while it dies. */
  cellOf(id: ActorId): { x: number; y: number } | undefined {
    return this.actors.get(id)?.cell
  }

  /** Moves every actor on to `now`; returns the hops that started. */
  update(now: number, yaw: number, pose: UprightPose, groundAt: (x: number, y: number) => number): ActorStep[] {
    const steps: ActorStep[] = []
    for (const [id, actor] of this.actors) {
      const wasMoving = actor.moving
      actor.update(now, yaw, pose, groundAt)
      for (const hop of actor.takeHops()) steps.push({ id, ...hop })
      if (actor.visual.castShadow && (wasMoving || actor.moving)) this.shadowChange = true
      if (actor.finished) this.remove(id, actor)
    }
    return steps
  }

  /** Whether a shadow of an actor changed since the last call: an actor that casts one moved, came or went. Breathing does not count. */
  takeShadowChange() {
    const changed = this.shadowChange
    this.shadowChange = false
    return changed
  }

  clear() {
    for (const [id, actor] of this.actors) this.remove(id, actor)
  }

  private remove(id: ActorId, actor: Actor) {
    this.shadowChange = true
    this.group.remove(actor.root)
    actor.dispose()
    this.actors.delete(id)
  }
}

/** How far an attack reaches at `t` (0 to 1): a short pull back, a fast strike, a slower return. */
function lungeReach(t: number) {
  if (t < WINDUP_END) return -Math.sin((Math.PI * t) / WINDUP_END / 2) * WINDUP_REACH
  if (t < STRIKE_END) {
    const u = (t - WINDUP_END) / (STRIKE_END - WINDUP_END)
    return -WINDUP_REACH + u * u * (LUNGE_REACH + WINDUP_REACH)
  }
  const u = (t - STRIKE_END) / (1 - STRIKE_END)
  return LUNGE_REACH * (1 - u) * (1 - u)
}
