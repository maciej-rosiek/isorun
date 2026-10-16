import { isWorn, type Worn } from './gear'
import { SLOT_COUNT, knownActionFrom, slotRefFrom, type KnownAction, type SlotRef } from './quick-slots'

/**
 * What the dock knows about a character that the server does not send on
 * its own: the spells and abilities its menus last listed, what its pack
 * said was worn, and what the player pinned on the quick slots. Kept in
 * `localStorage` by character, so a reload or a later session finds it.
 * A new game under the same name starts over (`turn` went back).
 */
export interface DockMemory {
  pins: (SlotRef | null)[]
  spells: KnownAction[]
  abilities: KnownAction[]
  worn: Map<string, Worn>
  turn: number
}

const KEY = 'isorun.dock'

export function emptyDockMemory(): DockMemory {
  return { pins: Array.from({ length: SLOT_COUNT }, () => null), spells: [], abilities: [], worn: new Map(), turn: 0 }
}

function readAll(): Record<string, unknown> {
  try {
    const raw = localStorage.getItem(KEY)
    const v: unknown = raw ? JSON.parse(raw) : {}
    return v && typeof v === 'object' && !Array.isArray(v) ? { ...v } : {}
  } catch {
    return {}
  }
}

/** The character's memory, or an empty one; `turn` is the game's turn now, and a turn earlier than the one kept means a new game. */
export function loadDockMemory(character: string, turn: number): DockMemory {
  const v = readAll()[character]
  const m = emptyDockMemory()
  if (!v || typeof v !== 'object') return m
  const kept = 'turn' in v && typeof v.turn === 'number' ? v.turn : 0
  if (turn < kept) return m
  m.turn = kept
  if ('pins' in v && Array.isArray(v.pins)) v.pins.slice(0, SLOT_COUNT).forEach((p: unknown, i: number) => (m.pins[i] = slotRefFrom(p)))
  if ('spells' in v && Array.isArray(v.spells)) m.spells = v.spells.flatMap((s: unknown) => knownActionFrom(s) ?? [])
  if ('abilities' in v && Array.isArray(v.abilities)) m.abilities = v.abilities.flatMap((s: unknown) => knownActionFrom(s) ?? [])
  if ('worn' in v && v.worn && typeof v.worn === 'object') {
    for (const [letter, w] of Object.entries(v.worn)) if (typeof w === 'string' && isWorn(w)) m.worn.set(letter, w)
  }
  return m
}

export function saveDockMemory(character: string, m: DockMemory) {
  const all = readAll()
  all[character] = { pins: m.pins, spells: m.spells, abilities: m.abilities, worn: Object.fromEntries(m.worn), turn: m.turn }
  try {
    localStorage.setItem(KEY, JSON.stringify(all))
  } catch {
    return
  }
}
