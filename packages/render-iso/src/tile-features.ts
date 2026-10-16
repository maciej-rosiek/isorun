import * as THREE from 'three/webgpu'
import { DIR8_DX, DIR8_DY, cellKey, getCell, type CellKey, type Scene, type SceneCell, type TileId, type TileSource } from '@isorun/scene'
import { doorTexture, type AtlasMaterials } from './materials.js'
import { ClayMaterial, castShadow } from './shading.js'
import { ATLAS_INSET, QuadBatch, addBox, type BoxSide, type Point3 } from './quads.js'

const POST_WIDTH = 0.18
const FRAME_DEPTH = 0.32
const LEAF_THICKNESS = 0.06
const LEAF_HEIGHT = 0.8
const DOOR_OPEN_ANGLE = -1.6
const DOOR_SWING_PER_SECOND = 7
const DOOR_STILL = 1e-4
const STEPS = 4
const STEP_DROP = 0.2
const STEP_RISE = 0.14
const PIT_BOTTOM = -1.2
const PIT_WALL_SHADE = 0.35
const RIM_WIDTH = 0.09
const RIM_HEIGHT = 0.07
const RIM_SHADE = 1.15

export function isTileFeature(cell: SceneCell | undefined): boolean {
  return cell?.feature?.type === 'door' || cell?.feature?.type === 'stairs'
}

type BoxMaker = (tile: TileId, min: Point3, max: Point3, shade: number, sides?: BoxSide[]) => void

interface Door {
  pivot: THREE.Object3D
  target: number
  cell: { x: number; y: number }
}

/** Doors and stairs built in 3D from the level's own tiles: a frame of the wall's stone with a swinging wooden leaf, and steps down into a pit or up onto a landing. */
export class TileFeatures {
  readonly group = new THREE.Group()
  private readonly doors = new Map<CellKey, Door>()
  /** How high the ground is where an actor stands on a cell: the middle step of stairs up, down in the pit of stairs down. */
  private readonly ground = new Map<CellKey, number>()
  private readonly geometries: THREE.BufferGeometry[] = []
  private readonly leafMaterial = new ClayMaterial({ map: doorTexture() })
  private leafGeometry: THREE.BoxGeometry | null = null
  private builtKey = ''
  /** Called when a door that was built stands open or shut where it was not before. */
  onDoor: ((x: number, y: number, open: boolean) => void) | null = null
  private builtMaterials: AtlasMaterials | null = null

  /** Returns whether the doors and stairs were built again. */
  build(scene: Scene, tiles: TileSource, materials: AtlasMaterials, wallHeight: number, forward: { x: number; y: number }): boolean {
    const features = [...scene.cells.values()].filter((c) => c.visibility !== 'unseen' && isTileFeature(c))
    const key = JSON.stringify([
      wallHeight,
      forward,
      scene.level.ceilingTile,
      features.map((c) => [c.x, c.y, c.feature, c.floorTile, c.featureTile, [0, 2, 4, 6].map((d) => getCell(scene, c.x + DIR8_DX[d], c.y + DIR8_DY[d])).map((n) => [n?.occluder, n?.wallTile])]),
    ])
    if (key === this.builtKey && materials === this.builtMaterials) return false
    this.builtKey = key
    this.builtMaterials = materials
    const before = new Map([...this.doors].map(([k, d]) => [k, { angle: d.pivot.rotation.y, target: d.target }]))
    this.clear()
    const batches = new Map<string, { batch: QuadBatch; material: THREE.Material }>()
    const batchFor = (tile: TileId) => {
      const rect = tiles.tile(tile)
      if (!rect) return null
      const placed = materials.forTile(rect)
      if (!placed) return null
      let entry = batches.get(placed.rect.atlas)
      if (!entry) batches.set(placed.rect.atlas, (entry = { batch: new QuadBatch(placed.width, placed.height, 0, ATLAS_INSET), material: placed.material }))
      return { batch: entry.batch, rect: placed.rect }
    }
    const box: BoxMaker = (tile, min, max, shade, sides) => {
      const target = batchFor(tile)
      if (target) addBox(target.batch, target.rect, min, max, shade, sides)
    }
    this.leafGeometry = new THREE.BoxGeometry(1 - 2 * POST_WIDTH, LEAF_HEIGHT * wallHeight, LEAF_THICKNESS).translate((1 - 2 * POST_WIDTH) / 2, (LEAF_HEIGHT * wallHeight) / 2, 0)

    for (const cell of scene.cells.values()) {
      if (cell.visibility === 'unseen') continue
      if (cell.feature?.type === 'door') this.addDoor(scene, cell, box, wallHeight, before.get(cellKey(cell.x, cell.y)))
      else if (cell.feature?.type === 'stairs') this.addStairs(scene, cell, box, cell.feature.dir, forward)
    }
    for (const { batch, material } of batches.values()) {
      const geometry = batch.geometry()
      this.geometries.push(geometry)
      const mesh = new THREE.Mesh(geometry, material)
      mesh.receiveShadow = true
      castShadow(mesh, true)
      this.group.add(mesh)
    }
    return true
  }

  /** The height of the ground an actor stands on in cell `x`, `y`: 0 off the stairs. */
  groundAt(x: number, y: number): number {
    return this.ground.get(cellKey(x, y)) ?? 0
  }

  /** Swing the doors; returns the cells of the doors that moved. */
  update(dt: number): { x: number; y: number }[] {
    const moved: { x: number; y: number }[] = []
    for (const door of this.doors.values()) {
      const r = door.pivot.rotation
      const turn = (door.target - r.y) * Math.min(1, dt * DOOR_SWING_PER_SECOND)
      r.y += turn
      if (Math.abs(turn) > DOOR_STILL) moved.push(door.cell)
    }
    return moved
  }

  dispose() {
    this.clear()
    this.leafMaterial.map?.dispose()
    this.leafMaterial.dispose()
  }

  private clear() {
    for (const g of this.geometries) g.dispose()
    this.geometries.length = 0
    this.leafGeometry?.dispose()
    this.leafGeometry = null
    this.group.clear()
    this.doors.clear()
    this.ground.clear()
  }

  private addDoor(scene: Scene, cell: SceneCell, box: BoxMaker, wallHeight: number, previous: { angle: number; target: number } | undefined) {
    const { x, y } = cell
    const west = getCell(scene, x - 1, y)
    const east = getCell(scene, x + 1, y)
    const north = getCell(scene, x, y - 1)
    const south = getCell(scene, x, y + 1)
    const walled = (c: SceneCell | undefined) => (c?.occluder ? 1 : 0)
    const across = walled(west) + walled(east)
    const along = walled(north) + walled(south)
    const runsEastWest = across > along || (across === along && across > 0)
    const neighbours = runsEastWest ? [west, east] : [north, south]
    const stone = neighbours.find((n) => n?.wallTile !== undefined)?.wallTile ?? scene.level.ceilingTile ?? cell.floorTile
    box(cell.floorTile, [x, -0.02, y], [x + 1, 0, y + 1], 1, ['top'])
    const h = wallHeight
    const half = FRAME_DEPTH / 2
    const lintel = LEAF_HEIGHT * h
    const someoneInside = scene.billboards.some((b) => b.x === x && b.y === y && (b.kind === 'player' || b.kind === 'monster'))
    if (runsEastWest) {
      const z0 = north?.occluder ? y : y + 0.5 - half
      const z1 = south?.occluder ? y + 1 : y + 0.5 + half
      box(stone, [x, 0, z0], [x + POST_WIDTH, h, z1], 1)
      box(stone, [x + 1 - POST_WIDTH, 0, z0], [x + 1, h, z1], 1)
      if (!someoneInside) box(stone, [x + POST_WIDTH, lintel, y + 0.5 - half], [x + 1 - POST_WIDTH, h, y + 0.5 + half], 1)
    } else {
      const x0 = west?.occluder ? x : x + 0.5 - half
      const x1 = east?.occluder ? x + 1 : x + 0.5 + half
      box(stone, [x0, 0, y], [x1, h, y + POST_WIDTH], 1)
      box(stone, [x0, 0, y + 1 - POST_WIDTH], [x1, h, y + 1], 1)
      if (!someoneInside) box(stone, [x + 0.5 - half, lintel, y + POST_WIDTH], [x + 0.5 + half, h, y + 1 - POST_WIDTH], 1)
    }
    if (!this.leafGeometry) return
    const leaf = new THREE.Mesh(this.leafGeometry, this.leafMaterial)
    castShadow(leaf, true)
    const pivot = new THREE.Group()
    pivot.add(leaf)
    const hinge = new THREE.Group()
    if (runsEastWest) hinge.position.set(x + POST_WIDTH, 0, y + 0.5)
    else hinge.position.set(x + 0.5, 0, y + POST_WIDTH)
    hinge.rotation.y = runsEastWest ? 0 : -Math.PI / 2
    hinge.add(pivot)
    this.group.add(hinge)
    const open = cell.feature?.type === 'door' && cell.feature.state === 'open'
    const target = open ? DOOR_OPEN_ANGLE : 0
    pivot.rotation.y = previous?.angle ?? target
    if (previous && previous.target !== target) this.onDoor?.(x, y, open)
    this.doors.set(cellKey(x, y), { pivot, target, cell: { x, y } })
  }

  /** Stairs run away from the camera: down into the screen, or up into it, so the eye looks onto the treads. */
  private addStairs(scene: Scene, cell: SceneCell, box: BoxMaker, dir: 'up' | 'down', forward: { x: number; y: number }) {
    const { x, y } = cell
    const away = [0, 2, 4, 6].reduce((best, d) => (DIR8_DX[d] * forward.x + DIR8_DY[d] * forward.y > DIR8_DX[best] * forward.x + DIR8_DY[best] * forward.y ? d : best))
    const fx = DIR8_DX[away]
    const fy = DIR8_DY[away]
    const stone = [0, 2, 4, 6].map((d) => getCell(scene, x + DIR8_DX[d], y + DIR8_DY[d])?.wallTile).find((t) => t !== undefined) ?? scene.level.ceilingTile ?? cell.floorTile
    /** A box in the stairs' own frame: u runs toward the wall the stairs face, v across, both 0..1. */
    const local = (u0: number, u1: number, v0: number, v1: number, y0: number, y1: number): [Point3, Point3] => {
      const at = (u: number, v: number) => ({ x: x + 0.5 + fx * (u - 0.5) - fy * (v - 0.5), y: y + 0.5 + fy * (u - 0.5) + fx * (v - 0.5) })
      const a = at(u0, v0)
      const b = at(u1, v1)
      return [
        [Math.min(a.x, b.x), y0, Math.min(a.y, b.y)],
        [Math.max(a.x, b.x), y1, Math.max(a.y, b.y)],
      ]
    }
    if (dir === 'down') {
      const [min, max] = local(0, 1, 0, 1, PIT_BOTTOM, 0)
      box(stone, min, max, PIT_WALL_SHADE, ['n', 's', 'w', 'e'])
      for (const [u0, u1, v0, v1] of [
        [0, 1, 0, RIM_WIDTH],
        [0, 1, 1 - RIM_WIDTH, 1],
        [1 - RIM_WIDTH, 1, 0, 1],
      ]) {
        const [rimMin, rimMax] = local(u0, u1, v0, v1, 0, RIM_HEIGHT)
        box(stone, rimMin, rimMax, RIM_SHADE)
      }
    }
    const middle = Math.floor(STEPS / 2)
    this.ground.set(cellKey(x, y), dir === 'down' ? -0.1 - middle * STEP_DROP : (middle + 1) * STEP_RISE)
    for (let i = 0; i < STEPS; i++) {
      const top = dir === 'down' ? -0.1 - i * STEP_DROP : (i + 1) * STEP_RISE
      const bottom = dir === 'down' ? PIT_BOTTOM : 0
      const shade = dir === 'down' ? 1 - i * 0.12 : 1
      const [min, max] = local(i / STEPS, (i + 1) / STEPS, 0, 1, bottom, top)
      box(dir === 'down' ? stone : cell.floorTile, min, max, shade, ['top'])
      box(stone, min, max, shade * 0.8, ['n', 's', 'w', 'e'])
    }
  }
}
