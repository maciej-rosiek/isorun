import { DIR8_DX, DIR8_DY, cellKey, dirFromDelta, getCell, keyToXY, type Billboard, type CellKey, type Dir8, type Scene } from '@isorun/scene'
import { IsoAssets, IsoCamera, IsoRender3d, TURN_DEGREES, actorId, stickToDir, type AimIntent, type IsoAim, type IsoRoute, type Vec2 } from '@isorun/render-iso'
import { parseDes, vaultScene } from '@isorun/vault'
import { loadRoomTiles } from '../src/room/tiles'
import des from './iso.des?raw'
import { lightsPanel } from '../src/lights-panel'
import { cameraPanel, storedAngles } from './camera-panel'
import { qualityPanel, storedQuality } from './quality-panel'

const TILT_PER_KEY = 5
const STICK_DEADZONE = 0.45
const REPEAT_DELAY_MS = 320
const REPEAT_EVERY_MS = 170
const MONSTER_TURN_DELAY_MS = 260
const MONSTER_SIGHT = 7
const MONSTER_HP = 2
const FPS_EVERY_MS = 500
const TRAVEL_STEP_MS = 170
const SIGHT_RADIUS_SQUARED = 50
const SIGHT_TARGETS: readonly [number, number][] = [[0.5, 0.5], [0.1, 0.1], [0.9, 0.1], [0.1, 0.9], [0.9, 0.9]]
const SIGHT_STEP = 0.1
const ZOOM_PER_WHEEL_PIXEL = 0.0015
const ZOOM_PER_KEY = 1.15

function element<T extends HTMLElement>(id: string, type: new () => T): T {
  const el = document.getElementById(id)
  if (!(el instanceof type)) throw new Error(`#${id} is not a ${type.name}`)
  return el
}

const canvas = element('view', HTMLCanvasElement)
const log = element('log', HTMLDivElement)
const padInfo = element('pad', HTMLDivElement)

const [tiles, assets] = await Promise.all([loadRoomTiles('/iso'), IsoAssets.load('/iso/models')])
const scene = vaultScene(parseDes(des), tiles)
const openDoor = [...scene.cells.values()].find((c) => c.feature?.type === 'door' && c.feature.state === 'open')
const hitPoints = new Map<number, number>()
const fogOfWar = new URLSearchParams(location.search).get('fog') !== 'off'
const explored = new Set<CellKey>()
prepareScene(scene)
updateSight()

const renderer = new IsoRender3d()
renderer.mount(canvas)
renderer.setAssets(assets)
renderer.setTiles(tiles)
renderer.setScene(scene)

const view = new IsoCamera(storedAngles())
view.snapTo(scene.player.x, scene.player.y)
const cameraControls = cameraPanel(element('camera', HTMLDivElement), view.angles, (angles) => view.setAngles(angles))
qualityPanel(element('graphics', HTMLDivElement), storedQuality(), (quality) => renderer.setQuality(quality))
lightsPanel(element('side', HTMLDivElement), new URLSearchParams(location.search).get('lights') ?? '', (lights) => renderer.setLights(lights))

function prepareScene(s: Scene) {
  let nextId = 1
  for (const b of s.billboards) {
    if (b.kind !== 'monster') continue
    b.scenery = false
    const id = nextId++
    b.ref = { id }
    hitPoints.set(id, MONSTER_HP)
  }
  const human = s.billboards.find((b) => b.name === 'human')
  if (!human) throw new Error('iso.des has no human (h) to play')
  human.kind = 'player'
  human.attitude = 'friendly'
  s.player = { x: human.x, y: human.y }
  for (const cell of s.cells.values()) {
    if (cell.glyph !== 'w' && cell.glyph !== 's') continue
    cell.flags.water = true
    cell.label = cell.glyph === 'w' ? 'deep water' : 'shallow water'
  }
}

// ------------------------------------------------------------------ the game stand-in

/** Whether a line from the player's centre to a point in a cell meets no wall before that cell. */
function clearLine(x: number, y: number, ox: number, oy: number) {
  const sx = scene.player.x + 0.5
  const sy = scene.player.y + 0.5
  const dx = x + ox - sx
  const dy = y + oy - sy
  const steps = Math.ceil(Math.hypot(dx, dy) / SIGHT_STEP)
  for (let i = 1; i < steps; i++) {
    const px = sx + (dx * i) / steps
    const py = sy + (dy * i) / steps
    const cx = Math.floor(px)
    const cy = Math.floor(py)
    if (cx === x && cy === y) return true
    if (blocksSight(cx, cy, px - cx, py - cy)) return false
  }
  return true
}

function isOccluder(x: number, y: number) {
  return getCell(scene, x, y)?.occluder === true
}

/** Whether a point in a cell stops the sight: an occluder stops it, except in a corner of the cell that has no occluder on either side, so the player sees through diagonal gaps. */
function blocksSight(x: number, y: number, fx: number, fy: number) {
  if (!isOccluder(x, y)) return false
  if (Math.abs(fx - 0.5) + Math.abs(fy - 0.5) <= 0.5) return true
  const sideX = fx < 0.5 ? -1 : 1
  const sideY = fy < 0.5 ? -1 : 1
  return isOccluder(x + sideX, y) || isOccluder(x, y + sideY)
}

/** Whether the player sees a cell: within the sight radius, with a clear line to its centre or to a point near one of its corners. */
function inSight(x: number, y: number) {
  const dx = x - scene.player.x
  const dy = y - scene.player.y
  if (dx * dx + dy * dy > SIGHT_RADIUS_SQUARED) return false
  return SIGHT_TARGETS.some(([ox, oy]) => clearLine(x, y, ox, oy))
}

/** Whether a wall touches an open cell in sight, also at a corner, so the walls in the corners of a room show. */
function bordersSight(x: number, y: number, inView: ReadonlySet<CellKey>) {
  return DIR8_DX.some((dx, i) => {
    const nx = x + dx
    const ny = y + DIR8_DY[i]
    return inView.has(cellKey(nx, ny)) && !isOccluder(nx, ny)
  })
}

/** Fog of war: cells in sight are visible, cells seen before are remembered, the rest unseen. */
function updateSight() {
  const inView = new Set<CellKey>()
  for (const cell of scene.cells.values()) if (!fogOfWar || inSight(cell.x, cell.y)) inView.add(cellKey(cell.x, cell.y))
  for (const cell of scene.cells.values()) {
    const key = cellKey(cell.x, cell.y)
    const visible = inView.has(key) || (cell.occluder && bordersSight(cell.x, cell.y, inView))
    if (visible) explored.add(key)
    cell.visibility = visible ? 'visible' : explored.has(key) ? 'remembered' : 'unseen'
  }
}

function say(text: string) {
  log.textContent = text
}

function monsterAt(x: number, y: number) {
  return scene.billboards.find((b) => b.kind === 'monster' && b.x === x && b.y === y)
}

function intentOf(dir: Dir8): AimIntent {
  const x = scene.player.x + DIR8_DX[dir]
  const y = scene.player.y + DIR8_DY[dir]
  if (monsterAt(x, y)) return 'attack'
  const cell = getCell(scene, x, y)
  if (!cell) return 'blocked'
  if (cell.kind === 'door') return 'move'
  return cell.occluder ? 'blocked' : 'move'
}

function publish(layoutChanged = false) {
  updateSight()
  scene.revision++
  if (layoutChanged) scene.layoutRevision++
  renderer.setScene(scene)
}

function monsterId(b: Billboard) {
  const id = actorId(b)
  return typeof id === 'number' ? id : undefined
}

function act(dir: Dir8) {
  const x = scene.player.x + DIR8_DX[dir]
  const y = scene.player.y + DIR8_DY[dir]
  const monster = monsterAt(x, y)
  const cell = getCell(scene, x, y)
  if (monster) {
    renderer.act('player', 'attack', { x, y })
    const id = monsterId(monster)
    const hp = (id !== undefined ? hitPoints.get(id) ?? 1 : 1) - 1
    if (id !== undefined) hitPoints.set(id, hp)
    if (hp > 0) {
      if (id !== undefined) renderer.act(id, 'hit', scene.player)
      say(`You hit the ${monster.name ?? 'monster'}.`)
    } else {
      if (id !== undefined) renderer.act(id, 'death', scene.player)
      scene.billboards.splice(scene.billboards.indexOf(monster), 1)
      say(`You kill the ${monster.name ?? 'monster'}!`)
    }
    publish()
  } else if (cell?.kind === 'door' && openDoor) {
    cell.kind = 'feature'
    cell.occluder = false
    cell.wallTile = undefined
    cell.featureTile = openDoor.featureTile
    cell.feature = openDoor.feature
    cell.stance = openDoor.stance
    say('You open the door.')
    publish(true)
  } else if (!cell || cell.occluder) {
    say(`There is a ${cell?.label ?? 'wall'} in the way.`)
    return
  } else {
    const player = scene.billboards.find((b) => b.kind === 'player')
    if (player) {
      player.x = x
      player.y = y
    }
    scene.player = { x, y }
    say(cell.kind === 'feature' && cell.label ? `There is a ${cell.label} here.` : '')
    publish()
  }
  setTimeout(monstersTurn, MONSTER_TURN_DELAY_MS)
}

function walkable(x: number, y: number) {
  const cell = getCell(scene, x, y)
  return cell !== undefined && !cell.occluder && !cell.flags.water && !monsterAt(x, y) && !(scene.player.x === x && scene.player.y === y)
}

function monstersTurn() {
  let moved = false
  for (const m of scene.billboards) {
    if (m.kind !== 'monster') continue
    const dx = scene.player.x - m.x
    const dy = scene.player.y - m.y
    const distance = Math.max(Math.abs(dx), Math.abs(dy))
    if (distance > MONSTER_SIGHT) continue
    const id = monsterId(m)
    if (distance === 1) {
      if (id !== undefined) renderer.act(id, 'attack', scene.player)
      renderer.act('player', 'hit', m)
      say(`The ${m.name ?? 'monster'} hits you.`)
      continue
    }
    const sx = Math.sign(dx)
    const sy = Math.sign(dy)
    const options: [number, number][] = [[sx, sy], [sx, 0], [0, sy]]
    const step = options.find(([ox, oy]) => (ox !== 0 || oy !== 0) && walkable(m.x + ox, m.y + oy))
    if (!step) continue
    m.x += step[0]
    m.y += step[1]
    moved = true
  }
  if (moved) publish()
}

// ------------------------------------------------------------------ travel

function passable(x: number, y: number, target: Vec2) {
  const cell = getCell(scene, x, y)
  if (!cell || cell.visibility === 'unseen' || cell.flags.water || (cell.occluder && cell.kind !== 'door')) return false
  return !monsterAt(x, y) || (x === target.x && y === target.y)
}

/** The shortest way from the player to a cell, in eight directions as in DCSS, and of the shortest ways the one closest to the straight line; null on the player's own cell. */
function routeTo(target: Vec2): IsoRoute | null {
  const start = scene.player
  if (start.x === target.x && start.y === target.y) return null
  const blocked: IsoRoute = { steps: [], target, intent: 'blocked' }
  if (!passable(target.x, target.y, target)) return blocked
  const distance = new Map<CellKey, number>([[cellKey(target.x, target.y), 0]])
  const startKey = cellKey(start.x, start.y)
  const queue: Vec2[] = [target]
  for (let i = 0; i < queue.length && !distance.has(startKey); i++) {
    const from = queue[i]
    const d = distance.get(cellKey(from.x, from.y)) ?? 0
    for (let dir = 0; dir < 8; dir++) {
      const x = from.x + DIR8_DX[dir]
      const y = from.y + DIR8_DY[dir]
      const key = cellKey(x, y)
      if (distance.has(key) || (key !== startKey && !passable(x, y, target))) continue
      distance.set(key, d + 1)
      queue.push({ x, y })
    }
  }
  const total = distance.get(startKey)
  if (total === undefined) return blocked
  const lineX = target.x - start.x
  const lineY = target.y - start.y
  const offLine = (c: Vec2) => Math.abs((c.x - start.x) * lineY - (c.y - start.y) * lineX)
  const steps: Vec2[] = []
  let at: Vec2 = start
  for (let left = total - 1; left > 0; left--) {
    const options: Vec2[] = []
    for (let dir = 0; dir < 8; dir++) {
      const next = { x: at.x + DIR8_DX[dir], y: at.y + DIR8_DY[dir] }
      if (distance.get(cellKey(next.x, next.y)) === left) options.push(next)
    }
    const best = options.reduce((a, b) => (offLine(b) < offLine(a) || (offLine(b) === offLine(a) && Math.hypot(b.x - target.x, b.y - target.y) < Math.hypot(a.x - target.x, a.y - target.y)) ? b : a))
    steps.push(best)
    at = best
  }
  return { steps, target, intent: monsterAt(target.x, target.y) ? 'attack' : 'move' }
}

function monstersInSight() {
  return scene.billboards.filter((b) => b.kind === 'monster' && getCell(scene, b.x, b.y)?.visibility === 'visible').length
}

let hover: Vec2 | null = null
let travel: { target: Vec2; monstersInSight: number } | null = null
let lastTravelAt = 0

canvas.addEventListener('pointermove', (e) => {
  if (drag) dragView(e)
  const key = renderer.pick(e.offsetX, e.offsetY)
  hover = key === null ? null : keyToXY(key)
})
canvas.addEventListener('pointerleave', () => (hover = null))
canvas.addEventListener('click', () => {
  if (hover) travel = { target: hover, monstersInSight: monstersInSight() }
})

/** One step of the travel the mouse started; it stops on the target (after opening it, for a door), after one attack, on a blocked way, or when a new monster comes into view. */
function travelStep(now: number) {
  if (!travel || now - lastTravelAt < TRAVEL_STEP_MS) return
  const route = routeTo(travel.target)
  if (!route || route.intent === 'blocked') {
    travel = null
    return
  }
  if (monstersInSight() > travel.monstersInSight) {
    travel = null
    say('A monster comes into view.')
    return
  }
  const next = route.steps[0] ?? route.target
  const dir = dirFromDelta(next.x - scene.player.x, next.y - scene.player.y)
  if (dir === null) {
    travel = null
    return
  }
  act(dir)
  lastTravelAt = now
  if (route.steps.length === 0 && route.intent === 'attack') travel = null
}

// ------------------------------------------------------------------ zoom and drag

let drag: { x: number; y: number } | null = null

/** Move the view with the map under the mouse; the view goes back to the player when the player moves. */
function dragView(e: PointerEvent) {
  if (!drag) return
  view.drag(e.clientX - drag.x, e.clientY - drag.y)
  drag = { x: e.clientX, y: e.clientY }
}

canvas.addEventListener('wheel', (e) => {
  e.preventDefault()
  view.zoomBy(Math.exp(-e.deltaY * ZOOM_PER_WHEEL_PIXEL))
}, { passive: false })
canvas.addEventListener('contextmenu', (e) => e.preventDefault())
canvas.addEventListener('pointerdown', (e) => {
  if (e.button !== 1 && e.button !== 2) return
  e.preventDefault()
  drag = { x: e.clientX, y: e.clientY }
  canvas.setPointerCapture(e.pointerId)
})
canvas.addEventListener('pointerup', (e) => {
  if (e.button === 1 || e.button === 2) drag = null
})

// ------------------------------------------------------------------ input

const keysDown = new Set<string>()
const KEY_VECTORS: Record<string, [number, number]> = {
  ArrowUp: [0, -1], KeyW: [0, -1],
  ArrowDown: [0, 1], KeyS: [0, 1],
  ArrowLeft: [-1, 0], KeyA: [-1, 0],
  ArrowRight: [1, 0], KeyD: [1, 0],
}
const CONFIRM_KEYS = new Set(['Space', 'Enter'])

let confirmHeld = false
let confirmPressedAt = 0
let lastRepeatAt = 0
const previousButtons: boolean[] = []

window.addEventListener('keydown', (e) => {
  if (e.target instanceof HTMLInputElement) return
  if (e.code === 'KeyQ') turn(-TURN_DEGREES)
  if (e.code === 'KeyE') turn(TURN_DEGREES)
  if (e.code === 'KeyR') tiltBy(TILT_PER_KEY)
  if (e.code === 'KeyF') tiltBy(-TILT_PER_KEY)
  if (e.code === 'Equal' || e.code === 'NumpadAdd') view.zoomBy(ZOOM_PER_KEY)
  if (e.code === 'Minus' || e.code === 'NumpadSubtract') view.zoomBy(1 / ZOOM_PER_KEY)
  if (KEY_VECTORS[e.code] || CONFIRM_KEYS.has(e.code)) e.preventDefault()
  keysDown.add(e.code)
})
window.addEventListener('keyup', (e) => keysDown.delete(e.code))
window.addEventListener('blur', () => keysDown.clear())

function pressed(gamepad: Gamepad | null, index: number) {
  return gamepad?.buttons[index]?.pressed ?? false
}

function firstGamepad(): Gamepad | null {
  return navigator.getGamepads().find((g) => g !== null) ?? null
}

/** The screen-space aim vector from the stick, the d-pad and the keys; null inside the dead zone. */
function aimVector(gamepad: Gamepad | null): [number, number] | null {
  let x = 0
  let y = 0
  for (const code of keysDown) {
    const v = KEY_VECTORS[code]
    if (v) {
      x += v[0]
      y += v[1]
    }
  }
  if (gamepad) {
    const sx = gamepad.axes[0] ?? 0
    const sy = gamepad.axes[1] ?? 0
    if (Math.hypot(sx, sy) > STICK_DEADZONE) {
      x += sx
      y += sy
    }
    if (pressed(gamepad, 12)) y -= 1
    if (pressed(gamepad, 13)) y += 1
    if (pressed(gamepad, 14)) x -= 1
    if (pressed(gamepad, 15)) x += 1
  }
  return Math.hypot(x, y) > 0.01 ? [x, y] : null
}

function poll(now: number) {
  const gamepad = firstGamepad()
  padInfo.textContent = gamepad ? `Gamepad: ${gamepad.id}` : 'No gamepad: press a button on it to connect.'
  const edge = (index: number) => {
    const down = pressed(gamepad, index)
    const was = previousButtons[index] ?? false
    previousButtons[index] = down
    return down && !was
  }
  if (edge(4)) turn(-TURN_DEGREES)
  if (edge(5)) turn(TURN_DEGREES)

  const vector = aimVector(gamepad)
  if (vector) travel = null
  const dir = vector ? stickToDir(vector[0], vector[1], view.yawGoal) : null
  const aim: IsoAim | null = dir === null ? null : { dir, intent: intentOf(dir) }

  const confirmDown = pressed(gamepad, 0) || [...CONFIRM_KEYS].some((k) => keysDown.has(k))
  if (confirmDown && !confirmHeld) confirmPressedAt = now
  const confirmStarted = confirmDown && !confirmHeld
  confirmHeld = confirmDown

  if (dir !== null) {
    const repeating = confirmDown && now - confirmPressedAt > REPEAT_DELAY_MS && now - lastRepeatAt > REPEAT_EVERY_MS
    if (confirmStarted || repeating) {
      act(dir)
      lastRepeatAt = now
    }
  }
  return aim
}

// ------------------------------------------------------------------ camera

function turn(degrees: number) {
  view.turn(degrees)
  cameraControls.show(view.angles)
}

function tiltBy(degrees: number) {
  view.tiltBy(degrees)
  cameraControls.show(view.angles)
}

function camera(dt: number) {
  view.slowFollow = travel !== null
  view.update(dt, scene.player.x, scene.player.y)
  renderer.setView(view.view)
  return view.camera
}

// ------------------------------------------------------------------ loop

function fit() {
  renderer.resize(canvas.clientWidth, canvas.clientHeight, window.devicePixelRatio || 1)
}

let last = performance.now()
const fps = element('fps', HTMLDivElement)
let fpsFrames = 0
let fpsSince = performance.now()

function countFrame(now: number) {
  fpsFrames++
  if (now - fpsSince < FPS_EVERY_MS) return
  fps.textContent = `${Math.round((fpsFrames * 1000) / (now - fpsSince))} fps`
  fpsFrames = 0
  fpsSince = now
}

function frame(now: number) {
  const dt = Math.min(0.1, (now - last) / 1000)
  last = now
  countFrame(now)
  const aim = poll(now)
  travelStep(now)
  const cam = camera(dt)
  renderer.setCamera(cam)
  renderer.setAim(aim)
  const pointed = travel?.target ?? hover
  renderer.setRoute(aim === null && pointed ? routeTo(pointed) : null)
  renderer.render()
  requestAnimationFrame(frame)
}

window.addEventListener('resize', fit)
fit()
requestAnimationFrame(frame)

Object.assign(window, { scene, publish, act, renderer })
