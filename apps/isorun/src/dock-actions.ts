import { DIR8_DX, DIR8_DY, cellUnder, getCell, type Scene } from '@isorun/scene'
import { formattedStringToText } from '@isorun/webtiles'
import type { Action } from './bindings'
import type { Context } from './context'
import type { KeyHint } from './tooltip'

/**
 * The dock's action icons: what can be done where the player stands, each
 * a button with crawl's own icon, so nothing hides behind a key. The ones
 * the square offers (the stairs, a shop, an altar, a pile to pick up, an open
 * door to close) come first and are lit; the standing ones (explore or
 * autofight, rest, wait, travel, search) follow.
 */

export interface DockIcon {
  /** a GUI tile by name (crawl's command icons, `CMD_*`) */
  gui?: string
  /** a dungeon feature tile by name */
  dngn?: string
  /** a tile by its global id: the feature underfoot */
  tile?: number
}

export interface DockAction {
  id: string
  label: string
  icon: DockIcon
  action: Action
  /** offered by the square the player stands on, not always there */
  here: boolean
  key: KeyHint
}

const keys = (label: string, text: string): Action => ({ kind: 'keys', label, seq: [{ text }] })
const control = (label: string, key: number): Action => ({ kind: 'keys', label, seq: [{ key }] })

/** The actions for the dock in command mode, in the order they stand. */
export function dockActions(ctx: Context, scene: Scene): DockAction[] {
  const out: DockAction[] = []
  const cell = scene.playerOnLevel ? cellUnder(scene) : undefined
  const u = ctx.under
  if (u.kind === 'feature' && cell?.featureTile !== undefined) {
    const f = u.feature
    const icon = { tile: cell.featureTile }
    if (f.type === 'stairs' || f.type === 'hatch') {
      const down = f.dir === 'down'
      const label = f.type === 'stairs' && f.exit ? 'Leave the dungeon' : down ? 'Go down' : 'Go up'
      out.push({ id: 'stairs', label, icon, action: keys(label, down ? '>' : '<'), here: true, key: { keys: [down ? '>' : '<'], pad: 'A' } })
    } else if (f.type === 'shop') out.push({ id: 'shop', label: 'Enter ' + u.label, icon, action: keys('Enter shop', '>'), here: true, key: { keys: ['>'], pad: 'A' } })
    else if (f.type === 'altar') out.push({ id: 'altar', label: 'Pray at ' + u.label, icon, action: keys('Pray', '>'), here: true, key: { keys: ['>'], pad: 'A' } })
    else if (f.type === 'portal' || f.type === 'transporter') out.push({ id: 'portal', label: 'Enter ' + u.label, icon, action: keys('Enter', '>'), here: true, key: { keys: ['>'], pad: 'A' } })
  }
  const pile = u.kind === 'item' ? u.label : u.kind === 'feature' ? u.items : undefined
  if (pile) out.push({ id: 'pickup', label: 'Pick up ' + formattedStringToText(pile), icon: { gui: 'CMD_DROP' }, action: keys('Pick up', ','), here: true, key: { keys: [','], pad: u.kind === 'item' ? 'A' : undefined } })
  if (scene.playerOnLevel && openDoorNextTo(scene)) out.push({ id: 'door', label: 'Close door', icon: { dngn: 'DNGN_OPEN_DOOR' }, action: keys('Close door', 'C'), here: true, key: { keys: ['C'] } })
  if (ctx.hostilesInView > 0) out.push({ id: 'fight', label: 'Autofight', icon: { gui: 'CMD_AUTOFIGHT' }, action: control('Autofight', 9), here: false, key: { keys: ['Tab'], pad: 'RT' } })
  else out.push({ id: 'explore', label: 'Explore', icon: { gui: 'CMD_EXPLORE' }, action: keys('Explore', 'o'), here: false, key: { keys: ['o'], pad: 'LT' } })
  out.push(
    { id: 'rest', label: 'Rest', icon: { gui: 'CMD_REST' }, action: keys('Rest', '5'), here: false, key: { keys: ['5'], pad: 'LB', padHold: true } },
    { id: 'wait', label: 'Wait a turn', icon: { gui: 'CMD_WAIT' }, action: keys('Wait', '.'), here: false, key: { keys: ['.'], pad: 'LB' } },
  )
  const onStairs = out.some((a) => a.id === 'stairs')
  const known = onStairs ? { up: false, down: false } : knownStairs(scene)
  if (known.up) out.push({ id: 'travel-up', label: 'Travel up the stairs', icon: { dngn: 'DNGN_STONE_STAIRS_UP' }, action: travelOneLevel('Travel up', '<'), here: false, key: { keys: ['G', '<'] } })
  if (known.down) out.push({ id: 'travel-down', label: 'Travel down the stairs', icon: { dngn: 'DNGN_STONE_STAIRS_DOWN' }, action: travelOneLevel('Travel down', '>'), here: false, key: { keys: ['G', '>'] } })
  out.push(
    { id: 'travel', label: 'Travel to a level', icon: { gui: 'CMD_INTERLEVEL_TRAVEL' }, action: keys('Travel', 'G'), here: false, key: { keys: ['G'] } },
    { id: 'search', label: 'Find items and places', icon: { gui: 'CMD_SEARCH_STASHES' }, action: control('Search', 6), here: false, key: { keys: ['Ctrl', 'F'] } },
  )
  return out
}

function travelOneLevel(label: string, key: '<' | '>'): Action {
  return { kind: 'keys', label, seq: [{ text: 'G' }, { text: key, await: 'prompt' }] }
}

function knownStairs(scene: Scene): { up: boolean; down: boolean } {
  let up = false
  let down = false
  for (const cell of scene.cells.values()) {
    const f = cell.feature
    if (f?.type !== 'stairs') continue
    if (f.dir === 'up' && !f.exit) up = true
    else if (f.dir === 'down' && !f.branch) down = true
    if (up && down) break
  }
  return { up, down }
}

function openDoorNextTo(scene: Scene): boolean {
  const { x, y } = scene.player
  for (let d = 0; d < 8; d++) {
    const f = getCell(scene, x + DIR8_DX[d], y + DIR8_DY[d])?.feature
    if (f?.type === 'door' && f.state === 'open') return true
  }
  return false
}
