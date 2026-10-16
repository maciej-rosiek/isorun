import type { CombatEvent, Fighter } from '../combat'
import type { AmbientName, Mood, SoundName } from './catalog'

export interface Cell {
  x: number
  y: number
}

/** A sound a fight asks for, when the blow shows on the screen and `after` seconds more; `at` is where it is heard from, the player's own ear when unset. */
export interface Cue {
  sound: SoundName
  at?: Cell
  after?: number
  gain?: number
}

/** What the player spent a turn on, found by the key that asked for it. */
export type UseKind = 'potion' | 'scroll' | 'cast'

/** How far a sound carries, in cells. */
const HEARING = 12
/** The least of a sound that is still heard, the far side of the hearing. */
const FAINTEST = 0.15
/** How far to one ear a sound at the edge of the screen goes, 1 being all of it. */
const WIDEST_PAN = 0.65
/** The part of the hit points under which the heart beats. */
const HEARTBEAT_BELOW = 0.35

const DARK_PLACES = new Set(['crypt', 'cr', 'tomb', 'hell', 'hel', 'dis', 'geh', 'gehenna', 'coc', 'cocytus', 'tar', 'tartarus', 'abyss', 'pan', 'pandemonium', 'zot', 'depths', 'dep', 'vaults', 'vault', 'slime', 'sl'])
const CAVE_PLACES = new Set(['lair', 'orc', 'mines', 'swamp', 'spider', 'snake', 'shoals', 'slime', 'sl', 'abyss', 'dis', 'geh', 'coc', 'tar', 'hell', 'hel'])
const DARK_FROM_DEPTH = 12

const USE_KEYS: Record<string, UseKind> = { q: 'potion', r: 'scroll', z: 'cast', Z: 'cast', v: 'cast' }
const REFUSAL = /^(okay, then|never mind|you (?:have|don't have|do not have) (?:no|nothing)|you (?:cannot|can't|are unable)|you can only|that is not|there is nothing)/i
const SCHOOLS: [RegExp, SoundName][] = [
  [/fire|flame|burn|blaze|inferno|scorch|sear|ignit/i, 'cast-fire'],
  [/frost|ice|icy|freez|cold|chill|glaci/i, 'cast-ice'],
  [/drain|death|necro|pain|corpse|dead|bone|vamp|torment|curse/i, 'cast-dark'],
]

/** The sound for a message line on its own: gold, a pickup, an experience level, the player's death. */
export function soundForLine(text: string): SoundName | undefined {
  if (/^you (?:now have \d+|pick up \d+) gold/i.test(text)) return 'gold'
  if (/^you die\b/i.test(text)) return 'gameover'
  if (/^welcome to level|^you have reached (?:level|xl)|^you feel more experienced/i.test(text)) return 'levelup'
  if (/^[a-zA-Z] - /.test(text)) return 'pickup'
  return undefined
}

/** The kind of use a key sent in the command mode starts, if it starts one. */
export function useKindOfKey(key: string): UseKind | undefined {
  return USE_KEYS[key]
}

/** The sound for the first answer to a use: not a refusal, and for a spell the school the lines name. */
export function soundForUse(kind: UseKind, lines: readonly string[]): SoundName | undefined {
  const first = lines[0]
  if (first === undefined || REFUSAL.test(first)) return undefined
  if (kind === 'potion') return 'potion'
  if (kind === 'scroll') return 'scroll'
  return SCHOOLS.find(([words]) => lines.some((line) => words.test(line)))?.[1] ?? 'cast'
}

/** What a fight message sounds like: an attack's swing, or the blow that lands. */
export function cuesForCombat(event: CombatEvent, cellOf: (who: Fighter) => Cell | undefined): Cue[] {
  if (event.kind === 'attack') return event.by === 'player' ? [{ sound: 'swing' }] : [{ sound: 'swing', at: cellOf(event.by), gain: 0.5 }]
  const at = event.on === 'player' ? undefined : cellOf(event.on)
  if (event.kind === 'miss') return [{ sound: 'miss', at, gain: event.on === 'player' ? 1 : 0.7 }]
  if (event.kind === 'death') return [{ sound: 'hit-heavy', at }, { sound: 'death', at, after: 0.12 }]
  if (event.on === 'player') return [{ sound: 'hurt' }, { sound: 'hit', gain: 0.6 }]
  return [{ sound: 'hit', at }]
}

/** How far to one side and how loud a sound at `source` is heard by `listener`; `across` is where it is on the screen, -1 at the left edge to 1 at the right. Undefined when it is out of hearing. */
export function hearing(listener: Cell, source: Cell, across: number): { pan: number; gain: number } | undefined {
  const distance = Math.max(Math.abs(listener.x - source.x), Math.abs(listener.y - source.y))
  if (distance > HEARING) return undefined
  return { pan: Math.max(-1, Math.min(1, across)) * WIDEST_PAN, gain: Math.max(FAINTEST, 1 - distance / HEARING) }
}

/** The music a place calls for. */
export function moodFor(place: string, depth: number): Mood {
  const branch = place.toLowerCase()
  if (DARK_PLACES.has(branch)) return 'dark'
  return depth >= DARK_FROM_DEPTH && (branch === 'dungeon' || branch === 'd') ? 'dark' : 'calm'
}

/** The loops that sound under a place: its bed, and the water and lava near the player (cells of each in reach). */
export function ambientLevels(place: string, water: number, lava: number): Partial<Record<AmbientName, number>> {
  const cave = CAVE_PLACES.has(place.toLowerCase())
  return { dungeon: cave ? 0 : 1, cavern: cave ? 1 : 0, water: Math.min(1, water / 10), lava: Math.min(1, lava / 10) }
}

/** Seconds from one heartbeat to the next at this part of the hit points, or undefined when the heart is calm. */
export function heartbeatDelay(hpFraction: number): number | undefined {
  if (hpFraction <= 0 || hpFraction >= HEARTBEAT_BELOW) return undefined
  return 0.5 + (hpFraction / HEARTBEAT_BELOW) * 0.9
}
