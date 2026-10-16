import { LIGHTS, QUALITY, isLightsName } from '@isorun/render-iso'
import { audio } from './audio'
import { RESOLUTIONS, TORCH_SHADOWS, surfaceDetails } from './graphics'
import { defaultSettings, getSettings, saveSettings, type Settings } from './servers'

/**
 * Isorun's own settings, one row each: what it is called, which key it sets,
 * the values it cycles through (in the order left and right walk them) and
 * how a value reads. The same list draws the settings scroll on the front
 * end's map (lobby.ts) and the settings menu in a game (main.ts), so the two
 * never disagree.
 *
 * A row may depend on the other settings: its hint can read differently
 * under them and it can be off under them.
 * `rowHint` and `rowOff` resolve those; consumers redraw every row after any
 * change, since one row's change can move another's.
 */
/**
 * The groups the settings screen is laid out in, in order (the plan's "one
 * settings destination"), each with the line it prints under its tab: the
 * settings panel draws one group at a time, the way a game's options screen
 * has a tab per kind of setting, so no group is ever a scroll long.
 */
const SETTING_GROUPS = [
  ['Graphics', 'How the dungeon looks, and how much work each frame does.'],
  ['Controls', 'What the keys, the d-pad and the sticks do.'],
  ['Interface', 'What the HUD shows around the view, and how big it is.'],
  ['Audio', 'How loud the sounds, the ambience of the dungeon and the music are.'],
] as const
export type SettingGroup = (typeof SETTING_GROUPS)[number][0]

export interface SettingRow<K extends keyof Settings = keyof Settings> {
  label: string
  group: SettingGroup
  /** the key this row sets */
  key: K
  values: readonly Settings[K][]
  fmt(v: Settings[K]): string
  /** what the scroll says when the player stands on it (the front end's message line), or what it says under the current settings */
  hint: string | ((s: Settings) => string)
  /**
   * When the row does nothing under the current settings. Left and right
   * leave it alone and it is drawn dim, the way the game's menus draw an
   * item that cannot be taken; its value still reads, so the player sees
   * what it will be once it applies.
   */
  off?(s: Settings): boolean
}

function row<K extends keyof Settings>(group: SettingGroup, label: string, key: SettingRow<K>['key'], values: readonly Settings[K][], hint: SettingRow<K>['hint'], fmt: (v: Settings[K]) => string = String, off?: (s: Settings) => boolean): SettingRow<K> {
  return { group, label, key, values, fmt, hint, off }
}

/** What the scroll says on `r` under `s`. */
export function rowHint(r: SettingRow, s: Settings = getSettings()): string {
  return typeof r.hint === 'function' ? r.hint(s) : r.hint
}

/** Whether `r` does nothing under `s`. */
export function rowOff(r: SettingRow, s: Settings = getSettings()): boolean {
  return r.off?.(s) ?? false
}

/**
 * Minimap size (hud.md "Minimap"), in tiles across: how much of the level
 * the map shows. Every odd count from MINIMAP_TILES_MIN to MINIMAP_TILES_MAX,
 * two tiles a step, so the player always stands on the centre tile; the
 * default is 21, the sidebar column's width on the reference layout, as
 * minimap.js `fit_to` makes it. The map follows the player at the cell the
 * "Minimap tile size" row sets, so more tiles show more of the level rather
 * than smaller cells.
 */
const MINIMAP_TILES_MIN = 9
export const MINIMAP_TILES_MAX = 61
export const MINIMAP_TILES: readonly number[] = Array.from({ length: (MINIMAP_TILES_MAX - MINIMAP_TILES_MIN) / 2 + 1 }, (_, i) => MINIMAP_TILES_MIN + 2 * i)

/**
 * Minimap tile size, in css px: how big each of those tiles is drawn. The
 * default is 16, the cell the map has always followed the player at; the
 * stops go down to 8, near the 4px cell WebTiles' own auto-sized minimap
 * comes out at on the reference column (minimap.js `fit_to` fits all `gxm`
 * columns in, since it shows the whole level rather than following), and up
 * to 32, `tile_cell_pixels`, a full dungeon tile. The count of tiles stays what the "Minimap size"
 * row says, so a bigger cell is the same map drawn larger, until the map
 * meets the caps in `minimapBox` and loses tiles off the count.
 */
export const MINIMAP_CELLS: readonly number[] = [8, 10, 12, 14, 16, 20, 24, 28, 32]

const VOLUMES: readonly number[] = Array.from({ length: 11 }, (_, i) => i * 10)

function percent(v: number) {
  return v + '%'
}

/** Every row, whatever is offered; `SETTING_ROWS` is what the player sees. */
export const ALL_SETTING_ROWS: readonly SettingRow[] = [
  // Graphics
  row('Graphics', 'Quality', 'graphics', ['low', 'medium', 'high'], 'How much work each frame does: the pixels drawn, the soft shadows in the corners, the edges and the torch shadows. Lower is faster on a weak GPU. The rows under it can change one of them.', (v) => QUALITY[v].label),
  row('Graphics', 'Surface detail', 'surfaceDetail', surfaceDetails(), 'What the tiles show of their surface. Low: rim light, dirt and wear, and how each cell differs. Medium: the depth of the bricks and stones, gloss, wetness and glowing tiles too. High: sparkle, dust in the torch light and deep liquids too. A phone stops at Medium.', (v) => v[0].toUpperCase() + v.slice(1)),
  row('Graphics', 'Resolution', 'resolution', RESOLUTIONS, 'How many of the screen\'s pixels the dungeon is drawn with; the rest are filled in when the picture is scaled up. Fewer is much faster on a big screen. Auto takes the Quality\'s.', (v) => (v === 'auto' ? 'Auto' : v + '%')),
  row('Graphics', 'Ambient occlusion', 'ambientOcclusion', ['auto', 'off', 'low', 'medium', 'high'], 'The soft shadows in the corners and at the foot of the walls. Low draws them with fewer samples, High at the full size of the screen. Auto takes the Quality\'s.', (v) => (v === 'auto' ? 'Auto' : v[0].toUpperCase() + v.slice(1))),
  row('Graphics', 'Antialiasing', 'antialias', ['auto', 'off', 'on'], 'Smooths the edges of the walls by drawing the scene four times over; it costs about as much as the rest of the scene. Auto takes the Quality\'s.', (v) => (v === 'auto' ? 'Auto' : v === 'on' ? 'On' : 'Off')),
  row('Graphics', 'Torch shadows', 'torchShadows', TORCH_SHADOWS, 'How many of the wall torches nearest to the middle of the view cast shadows. Auto takes the Quality\'s.', (v) => (v === 'auto' ? 'Auto' : String(v))),
  row('Graphics', 'Animation speed', 'animationSpeed', [1, 1.25, 1.5, 2, 3], 'How fast the player and the monsters walk and fight on the screen. The game itself does not wait for them.', (v) => v + 'x'),
  row('Graphics', 'Travel', 'travel', ['paced', 'fast'], 'Paced: travel and explore wait for every hop, so the walk shows cell by cell. Fast: they go as fast as the server does, and a long way is a jump.', (v) => (v === 'paced' ? 'Paced' : 'Fast')),
  row('Graphics', 'Look', 'look', [...Object.keys(LIGHTS).filter(isLightsName), 'custom'], 'The lights, the colours and the screen effects of the dungeon. Custom is what Tune the look set last.', (v) => (v === 'custom' ? 'Custom' : LIGHTS[v].label)),
  // Controls
  row('Controls', 'Arrow keys', 'arrowKeys', ['aim', 'step'], 'Aim: the arrows point an arrow on the ground (two of them a diagonal) and Space or Enter steps that way. Step: every arrow steps at once.', (v) => (v === 'aim' ? 'Aim, then Space' : 'Step at once')),
  row('Controls', 'Left stick', 'leftStick', ['step', 'aim'], 'Step: the left stick steps at once. Aim: the stick points an arrow on the ground and A steps that way, again and again while held.', (v) => (v === 'aim' ? 'Aim, then A' : 'Step at once')),
  row('Controls', 'LT explore', 'exploreTrigger', ['tap', 'hold'], 'Tap: a tap of LT explores with every step shown, a hold explores at once. Hold: LT held explores with every step shown and letting go stops; a tap explores at once. While an explore walks, LT stops it.', (v) => (v === 'tap' ? 'Tap to walk, hold for at once' : 'Hold to walk, tap for at once')),
  row('Controls', 'Hints', 'hints', ['adaptive', 'contextual', 'off'], 'Gamepad prompts in the corner of the view for what is in front of you. Adaptive also teaches the controls until used, across all runs. Contextual only skips the teaching. Off hides gameplay hints; menu and targeting controls remain.', (v) => ({ adaptive: 'Adaptive', contextual: 'Contextual only', off: 'Off' })[v]),
  // Interface
  row('Interface', 'HUD', 'hud', ['dock', 'classic'], 'Dock: health and magic orbs, quick slots and a button for every screen along the foot of the screen, all usable with the mouse and the pad. Classic: the WebTiles panes, as the official client lays them out.', (v) => (v === 'dock' ? 'Dock' : 'Classic')),
  row('Interface', 'UI scale', 'uiScale', [1, 1.15, 1.3, 1.5, 0.85], 'The size of the HUD, menus and messages.', (v) => Math.round(v * 100) + '%'),
  row('Interface', 'Minimap size', 'minimapTiles', MINIMAP_TILES, 'How many tiles across the minimap shows: it grows out of its corner over the view, at the same cell size.', (v) => v + ' tiles'),
  row('Interface', 'Minimap tile size', 'minimapCell', MINIMAP_CELLS, 'How big each minimap tile is drawn: the same tiles, larger, until the map runs out of room over the view.', (v) => v + 'px'),
  row('Interface', 'Damage numbers', 'damageNumbers', [true, false], 'Numbers over the fighters as blows land: the damage dealt, and the damage taken in red, bigger the more of your life it took.', (v) => (v ? 'On' : 'Off')),
  row('Audio', 'Master volume', 'masterVolume', VOLUMES, 'How loud everything is.', percent),
  row('Audio', 'Effects volume', 'effectsVolume', VOLUMES, 'How loud the steps, the fights, the doors, the spells and the menus are.', percent),
  row('Audio', 'Ambient volume', 'ambientVolume', VOLUMES, 'How loud the dungeon itself is: drips and wind, water and lava near you.', percent),
  row('Audio', 'Music volume', 'musicVolume', VOLUMES, 'How loud the music is.', percent),
  row('Audio', 'Mute', 'muted', [false, true], 'Silences everything and keeps the volumes.', (v) => (v ? 'Muted' : 'Off')),
]

/** The offered rows by group, in group order, each with its tab's line; groups with no row left out. */
export function settingGroups(rows: readonly SettingRow[] = SETTING_ROWS): { group: SettingGroup; hint: string; rows: SettingRow[] }[] {
  return SETTING_GROUPS.map(([group, hint]) => ({ group, hint, rows: rows.filter((r) => r.group === group) })).filter((g) => g.rows.length)
}

export const SETTING_ROWS: readonly SettingRow[] = ALL_SETTING_ROWS

/** How `r` reads right now. */
export function settingValue(r: SettingRow, s: Settings = getSettings()): string {
  return (r.fmt as (v: unknown) => string)(s[r.key])
}

/**
 * Move `r` on by `d` steps (right is next, left is previous, wrapping),
 * save, and apply what applies at once (the UI scale). Returns how the new
 * value reads; a running game applies the rest through `onchange`. A row
 * that is off under the current settings (`rowOff`) is left as it is. A
 * value that is not one of the stops (a default between two, an old saved
 * value) steps to the next stop in that direction (`offStop`).
 */
export function adjustSetting(r: SettingRow, d: number, onchange?: () => void): string {
  const s = getSettings()
  if (rowOff(r, s)) return settingValue(r, s)
  const key = r.key
  const values = r.values as readonly unknown[]
  const cur = s[key]
  const i = values.indexOf(cur)
  const n = i >= 0 || typeof cur !== 'number' ? values[((((i < 0 ? 0 : i) + d) % values.length) + values.length) % values.length] : offStop(values as readonly number[], cur, d)
  ;(s as unknown as Record<string, unknown>)[key] = n
  saveSettings(s)
  document.documentElement.style.setProperty('--ui-scale', String(s.uiScale))
  audio.applySettings(s)
  onchange?.()
  return settingValue(r, s)
}

/** Whether every row `group` offers is at its default. */
export function groupAtDefaults(group: SettingGroup, s: Settings = getSettings()): boolean {
  return SETTING_ROWS.every((r) => r.group !== group || s[r.key] === defaultSettings[r.key])
}

/**
 * Put every row `group` offers back to its default, save, and apply what
 * applies at once (the UI scale), as `adjustSetting` does. A row the build
 * does not offer (`VIEW_OPTIONS`) keeps what it was.
 */
export function resetGroup(group: SettingGroup, onchange?: () => void) {
  const s = getSettings()
  for (const r of SETTING_ROWS) if (r.group === group) (s as unknown as Record<string, unknown>)[r.key] = defaultSettings[r.key]
  saveSettings(s)
  document.documentElement.style.setProperty('--ui-scale', String(s.uiScale))
  audio.applySettings(s)
  onchange?.()
}

/**
 * The stop `d` steps from `cur`, which is not itself a stop. On an ascending
 * scale (the camera heights) one step right is the first stop above and one
 * step left the first stop below, so 0.62 goes to 0.65 or 0.6 and no stop is
 * skipped; on any other scale it is the nearest stop, then the step.
 */
function offStop(values: readonly number[], cur: number, d: number): number {
  const ascending = values.every((v, k) => k === 0 || v > values[k - 1])
  if (ascending) {
    const above = values.findIndex((v) => v > cur)
    const i = d > 0 ? (above < 0 ? values.length : above) + d - 1 : (above < 0 ? values.length : above) + d
    return values[Math.max(0, Math.min(values.length - 1, i))]
  }
  let i = 0
  for (let k = 1; k < values.length; k++) if (Math.abs(values[k] - cur) < Math.abs(values[i] - cur)) i = k
  return values[((((i + d) % values.length) + values.length) % values.length)]
}
