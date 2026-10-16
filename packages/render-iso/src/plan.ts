import { cellKey, getCell, hasItemsUnder, yawToDir, type Billboard, type CellKey, type Dir8, type Scene, type SceneCell, type TileId, type TileRect, type TileSource } from '@isorun/scene'

export interface IsoView {
  /** Screen pixels per cell. */
  zoom: number
  /** Wall height in cells. */
  wallHeight: number
  /** The camera's field of view in degrees, top to bottom: 0 for the isometric view (parallel lines), more for a perspective view. */
  fov?: number
}

export const DEFAULT_VIEW: IsoView = { zoom: 68, wallHeight: 1 }

export type AimIntent = 'move' | 'attack' | 'blocked'

export const AIM_COLOURS: Record<AimIntent, string> = { move: '#ffd75e', attack: '#ff5a4a', blocked: '#9a9a9a' }

export interface IsoAim {
  dir: Dir8
  intent: AimIntent
}

export interface Vec2 {
  x: number
  y: number
}

/** A way to a cell: the cells to step on before it, and what happens on it. */
export interface IsoRoute {
  steps: Vec2[]
  target: Vec2
  intent: AimIntent
}

export interface Vec3 {
  x: number
  y: number
  z: number
}

/** Screen right and screen up as directions on the ground, for a yaw (0 = north is up the screen, clockwise). */
export function groundBasis(yaw: number): { right: Vec2; forward: Vec2 } {
  return { right: { x: Math.cos(yaw), y: Math.sin(yaw) }, forward: { x: Math.sin(yaw), y: -Math.cos(yaw) } }
}

/** The compass direction a screen-space vector (x right, y down) points at on the ground. */
export function stickToDir(x: number, y: number, yaw: number): Dir8 {
  const { right, forward } = groundBasis(yaw)
  const wx = x * right.x - y * forward.x
  const wy = x * right.y - y * forward.y
  return yawToDir(Math.atan2(wx, -wy))
}

export type Side = 'n' | 's' | 'w' | 'e'

/** Each side of a cell: its outward normal and its two ground corners, left to right as seen from outside. */
export const SIDES: Record<Side, { normal: Vec2; from: Vec2; to: Vec2 }> = {
  n: { normal: { x: 0, y: -1 }, from: { x: 1, y: 0 }, to: { x: 0, y: 0 } },
  s: { normal: { x: 0, y: 1 }, from: { x: 0, y: 1 }, to: { x: 1, y: 1 } },
  w: { normal: { x: -1, y: 0 }, from: { x: 0, y: 0 }, to: { x: 0, y: 1 } },
  e: { normal: { x: 1, y: 0 }, from: { x: 1, y: 1 }, to: { x: 1, y: 0 } },
}

const SIDE_NAMES: Side[] = ['n', 's', 'w', 'e']

export const CUTAWAY_HEIGHT = 0.2
const PLAYER_HALF_WIDTH = 0.5

export interface GroundLayer {
  x: number
  y: number
  tile: TileId
  /** Draw order above the floor, 0 for the floor itself. */
  lift: number
}

export interface WallFace {
  side: Side
  tile: TileId
}

export interface WallBlock {
  x: number
  y: number
  height: number
  top: TileId
  faces: WallFace[]
}

export interface SpriteLayer {
  tile: TileId
  ox: number
  oy: number
  ymax?: number
}

export interface Sprite {
  billboard?: Billboard
  x: number
  y: number
  layers: SpriteLayer[]
  lying: boolean
  /** The top item of a pile: more items lie under it. */
  pile?: true
}

export interface IsoPlan {
  ground: GroundLayer[]
  walls: WallBlock[]
  sprites: Sprite[]
}

/**
 * The cells walls must not hide: the player's, every floor cell seen within `reach` cells of it (a touch screen
 * has no hover to cut the walls away from the cell it aims at), and one more (the cell the mouse points at).
 */
export function cutAwayFocus(scene: Scene, extra: Vec2 | null, reach = 0): Vec2[] {
  const focus = scene.playerOnLevel ? [scene.player, ...floorAround(scene, scene.player, reach)] : []
  return extra ? [...focus, extra] : focus
}

function floorAround(scene: Scene, centre: Vec2, reach: number): Vec2[] {
  const out: Vec2[] = []
  for (let y = centre.y - reach; y <= centre.y + reach; y++)
    for (let x = centre.x - reach; x <= centre.x + reach; x++) {
      if ((x === centre.x && y === centre.y) || Math.hypot(x - centre.x, y - centre.y) > reach) continue
      if (isPointable(getCell(scene, x, y))) out.push({ x, y })
    }
  return out
}

/** Whether the wall at a cell stands between the camera and a focus cell, so it drops to a stub. */
export function isCutAway(x: number, y: number, focus: Vec2, yaw: number, elevation: number, wallHeight: number): boolean {
  const { forward, right } = groundBasis(yaw)
  const dx = x - focus.x
  const dy = y - focus.y
  const depth = dx * forward.x + dy * forward.y
  const across = dx * right.x + dy * right.y
  const halfAcross = (Math.abs(right.x) + Math.abs(right.y)) / 2
  const halfDepth = (Math.abs(forward.x) + Math.abs(forward.y)) / 2
  const wallTopOnScreen = (depth + halfDepth) * Math.sin(elevation) + wallHeight * Math.cos(elevation)
  return depth < 0 && Math.abs(across) < halfAcross + PLAYER_HALF_WIDTH && wallTopOnScreen > 0
}

/** The height of the wall at a cell as drawn: a stub when it hides a focus cell. */
export function wallHeightAt(x: number, y: number, focus: readonly Vec2[], yaw: number, elevation: number, view: IsoView) {
  return focus.some((f) => isCutAway(x, y, f, yaw, elevation, view.wallHeight)) ? Math.min(CUTAWAY_HEIGHT, view.wallHeight) : view.wallHeight
}

/** The wall cells that drop to stubs for these focus cells. */
export function cutAwayCells(scene: Scene, focus: readonly Vec2[], yaw: number, elevation: number, view: IsoView): Set<CellKey> {
  const reach = Math.ceil(view.wallHeight / Math.max(0.01, Math.tan(elevation))) + 2
  const cut = new Set<CellKey>()
  for (const f of focus)
    for (let y = f.y - reach; y <= f.y + reach; y++)
      for (let x = f.x - reach; x <= f.x + reach; x++) {
        const cell = getCell(scene, x, y)
        if (cell?.occluder && isCutAway(x, y, f, yaw, elevation, view.wallHeight)) cut.add(cellKey(x, y))
      }
  return cut
}

/** The wall cells that drop to stubs for these focus cells, as a key that changes only when that set changes. */
export function cutAwayKey(scene: Scene, focus: readonly Vec2[], yaw: number, elevation: number, view: IsoView): string {
  return [...cutAwayCells(scene, focus, yaw, elevation, view)].sort().join(' ')
}

/** The point where a view ray meets the ground, in cells. */
export function groundPoint(origin: Vec3, direction: Vec3): Vec2 | null {
  if (direction.y >= 0) return null
  const t = -origin.y / direction.y
  return { x: origin.x + direction.x * t, y: origin.z + direction.z * t }
}

/** Whether the mouse may point at a cell: seen, and open floor or a door. */
export function isPointable(cell: SceneCell | undefined): boolean {
  return cell !== undefined && cell.visibility !== 'unseen' && (!cell.occluder || cell.kind === 'door')
}

/** Whether a billboard shows: on a cell seen before, and a monster only in sight. */
export function isShown(b: Billboard, scene: Scene): boolean {
  const visibility = getCell(scene, b.x, b.y)?.visibility ?? 'visible'
  return b.kind === 'monster' ? visibility === 'visible' : visibility !== 'unseen'
}

/** Everything a renderer draws, in world space: what lies on the ground, the wall blocks and what stands; with `within`, only on the cells it takes. */
export function planScene(scene: Scene, yaw: number, elevation: number, view: IsoView, focus: readonly Vec2[], within: (x: number, y: number) => boolean = () => true): IsoPlan {
  const plan: IsoPlan = { ground: [], walls: [], sprites: [] }
  for (const cell of scene.cells.values()) {
    if (cell.visibility === 'unseen' || !within(cell.x, cell.y)) continue
    if (cell.occluder) {
      const top = cell.wallTile ?? cell.floorTile
      const faces: WallFace[] = []
      for (const side of SIDE_NAMES) {
        const { normal } = SIDES[side]
        const neighbour = getCell(scene, cell.x + normal.x, cell.y + normal.y)
        if (neighbour?.occluder && neighbour.visibility !== 'unseen') continue
        faces.push({ side, tile: cell.faceTiles?.[side] ?? top })
      }
      const height = wallHeightAt(cell.x, cell.y, focus, yaw, elevation, view)
      plan.walls.push({ x: cell.x, y: cell.y, height, top, faces })
      continue
    }
    const shadows = new Set(cell.wallShadows ?? [])
    const layers = [cell.floorTile, ...(cell.underlays ?? [])]
    if (cell.featureTile !== undefined && (cell.stance === 'lying' || cell.stance === 'decal')) layers.push(cell.featureTile)
    for (const id of cell.overlays ?? []) if (!shadows.has(id)) layers.push(id)
    layers.forEach((tile, lift) => plan.ground.push({ x: cell.x, y: cell.y, tile, lift }))
    if (cell.featureTile !== undefined && cell.stance === 'upright') {
      plan.sprites.push({ x: cell.x, y: cell.y, layers: [{ tile: cell.featureTile, ox: 0, oy: 0 }], lying: false })
    }
  }
  for (const b of scene.billboards) {
    if (!isShown(b, scene) || !within(b.x, b.y)) continue
    const layers = b.layers?.map((l) => ({ tile: l.tile, ox: l.ox ?? 0, oy: l.oy ?? 0, ymax: l.ymax })) ?? [{ tile: b.tile, ox: 0, oy: 0 }]
    const sprite: Sprite = { billboard: b, x: b.x, y: b.y, layers, lying: b.lying ?? false }
    if (b.kind === 'item' && hasItemsUnder(b)) sprite.pile = true
    plan.sprites.push(sprite)
  }
  return plan
}

/** Where a tile's art is drawn within its cell, in cells: left and top from the cell's top-left corner. */
export function tileFrame(rect: TileRect, ox = 0, oy = 0, ymax?: number) {
  const h = ymax !== undefined ? Math.max(0, Math.min(rect.h, ymax - rect.oy)) : rect.h
  return {
    left: (rect.ox + ox) / rect.cell,
    top: (rect.oy + oy) / rect.cell,
    width: rect.w / rect.cell,
    height: h / rect.cell,
    texelHeight: h,
  }
}

/** The pixel size of an atlas image, or null for a source a renderer cannot draw. */
export function atlasImage(tiles: TileSource, name: string): { image: Exclude<TexImageSource, VideoFrame | ImageData>; width: number; height: number } | null {
  const image = tiles.atlas(name)
  if (!image || image instanceof ImageData || !('width' in image)) return null
  return { image, width: image.width, height: image.height }
}

const TORCH_EVERY = 5

/** Whether a wall carries a torch on this face: about one face in five, the same every time. */
export function hasTorch(x: number, y: number, side: Side): boolean {
  let h = (x * 374761393 + y * 668265263 + (['n', 's', 'w', 'e'].indexOf(side) + 7) * 2147483647) | 0
  h = Math.imul(h ^ (h >>> 13), 1274126177)
  return (((h ^ (h >>> 16)) >>> 0) / 4294967296) * TORCH_EVERY < 1
}

export const TORCH_HEIGHT = 0.5
