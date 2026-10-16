import { formattedStringToText, type MenuState } from '@isorun/webtiles'
import type { PackItem } from './gear'

/**
 * The dock's quick slots: spells, abilities, the quivered shot and the
 * consumables, each one click (or one press of A on the pad) away.
 *
 * WebTiles sends no list of spells or abilities; crawl's own menus are the
 * only place they are named. Every time one of them is up, its rows are kept
 * (`spellsFromMenu`, `abilitiesFromMenu`), so the slots know them from then
 * on. A slot the player pinned keeps what was put on it; the others fill in
 * by themselves in a fixed order (`quickSlots`).
 */

export const SLOT_COUNT = 10

export interface TileLayer {
  t: number
  tex?: number
  ymax?: number
}

/** A spell or ability as crawl's menu listed it: its letter, name and icon. */
export interface KnownAction {
  letter: string
  name: string
  tiles: TileLayer[]
}

export type SlotRef = { kind: 'spell'; letter: string } | { kind: 'ability'; letter: string } | { kind: 'item'; slot: number } | { kind: 'fire' }

export interface QuickSlot {
  ref: SlotRef
  label: string
  tiles: TileLayer[]
  /** the stack's count, for a consumable */
  count?: number
  /** the item is of no use to this character (the server's `useless`) */
  useless?: boolean
  /** the player put it here; the rest fill in by themselves */
  pinned: boolean
  verb?: string
  letter?: string
}

export interface SlotSources {
  spells: readonly KnownAction[]
  abilities: readonly KnownAction[]
  /** the pack's consumables, in the action panel's order (webtiles `actionPanelItems`) */
  consumables: readonly PackItem[]
  items: readonly PackItem[]
  /** crawl's description of the quivered action, when there is one ("Fire: 3 stones") */
  quiver: string
  /** the item the quiver fires, when it fires one (`quiver_item`) */
  quiverItem?: PackItem
}

/** spl-cast.cc and ability.cc rows: the letter, a marker, then the name up to the next run of spaces. */
const KNOWN_ROW_RE = /^\s*([a-zA-Z])\s+[-+#]\s+(.+?)(?:\s{2,}|$)/

/** The spells crawl's lists of known spells hold (`z *`, `I`: "Your spells (cast)", "Your spells (describe)"); the memorise menu shares their tag and lists others. */
export function spellsFromMenu(menu: Pick<MenuState, 'tag' | 'items' | 'title'>): KnownAction[] | null {
  return menu.tag === 'spell' && formattedStringToText(menu.title?.text ?? '').trimStart().startsWith('Your spells') ? knownRows(menu) : null
}

/** The abilities crawl's `a` menu lists: its tag is `ability`. */
export function abilitiesFromMenu(menu: Pick<MenuState, 'tag' | 'items'>): KnownAction[] | null {
  return menu.tag === 'ability' ? knownRows(menu) : null
}

function knownRows(menu: Pick<MenuState, 'items'>): KnownAction[] {
  const out: KnownAction[] = []
  for (const item of menu.items) {
    if (!item?.hotkeys?.length) continue
    const m = KNOWN_ROW_RE.exec(formattedStringToText(item.text))
    if (!m) continue
    out.push({ letter: m[1], name: m[2].trim(), tiles: (item.tiles ?? []).map(({ t, tex, ymax }) => ({ t, tex, ymax })) })
  }
  return out
}

function sameRef(a: SlotRef, b: SlotRef): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}

/** A slot for what `ref` points at now, or null when it is gone (the spell forgotten, the last potion drunk). */
function slotFor(ref: SlotRef, src: SlotSources, pinned: boolean): QuickSlot | null {
  if (ref.kind === 'fire') {
    if (!src.quiver) return null
    return { ref, label: src.quiver, tiles: src.quiverItem ? itemTiles(src.quiverItem) : [], count: src.quiverItem?.quantity, pinned }
  }
  if (ref.kind === 'item') {
    const item = src.items.find((i) => i.slot === ref.slot)
    if (!item) return null
    const qty = typeof item.qty_field === 'string' ? item[item.qty_field] : item.quantity
    return { ref, label: typeof item.name === 'string' ? item.name : item.letter, tiles: itemTiles(item), count: typeof qty === 'number' ? qty : undefined, useless: item.useless === true, pinned, verb: typeof item.action_verb === 'string' ? item.action_verb : undefined, letter: item.letter }
  }
  const list = ref.kind === 'spell' ? src.spells : src.abilities
  const known = list.find((k) => k.letter === ref.letter)
  return known ? { ref, label: known.name, tiles: known.tiles, pinned } : null
}

/** An item's icon layers: tileweb.cc `_send_item` sends plain ids in the main texture, the known base item first. */
export function itemTiles(item: Pick<PackItem, 'tile'>): TileLayer[] {
  const tile = item.tile
  if (tile === undefined) return []
  return (Array.isArray(tile) ? tile : [tile]).map((t) => (typeof t === 'number' ? { t, tex: MAIN_TEXTURE } : { t: t.t, tex: t.tex, ymax: t.ymax }))
}

/** tiles.h `TEX_DEFAULT`, the main atlas, in the server's texture numbering */
const MAIN_TEXTURE = 4

/**
 * The slots, SLOT_COUNT of them: each pinned one where the player put it
 * (empty if what it held is gone), the rest filled in order with the
 * quivered shot, the spells, the abilities and then the consumables.
 */
export function quickSlots(pins: readonly (SlotRef | null)[], src: SlotSources): (QuickSlot | null)[] {
  const out: (QuickSlot | null)[] = Array.from({ length: SLOT_COUNT }, (_, i) => {
    const ref = pins[i]
    return ref ? slotFor(ref, src, true) : null
  })
  const taken = pins.filter((p): p is SlotRef => !!p)
  const fire: SlotRef = { kind: 'fire' }
  const auto: SlotRef[] = [
    fire,
    ...src.spells.map((s): SlotRef => ({ kind: 'spell', letter: s.letter })),
    ...src.abilities.map((a): SlotRef => ({ kind: 'ability', letter: a.letter })),
    ...src.consumables.map((c): SlotRef => ({ kind: 'item', slot: c.slot })),
  ].filter((r) => !taken.some((t) => sameRef(t, r)))
  let next = 0
  for (let i = 0; i < SLOT_COUNT; i++) {
    if (pins[i]) continue
    while (next < auto.length && !out[i]) out[i] = slotFor(auto[next++], src, false)
  }
  return out
}

/** Everything a slot can hold right now, for the chooser that pins one. */
export function slotChoices(src: SlotSources): { ref: SlotRef; label: string; group: string; tiles: TileLayer[] }[] {
  const out: { ref: SlotRef; label: string; group: string; tiles: TileLayer[] }[] = []
  if (src.quiver) out.push({ ref: { kind: 'fire' }, label: src.quiver, group: 'Quiver', tiles: src.quiverItem ? itemTiles(src.quiverItem) : [] })
  for (const s of src.spells) out.push({ ref: { kind: 'spell', letter: s.letter }, label: s.name, group: 'Spell', tiles: s.tiles })
  for (const a of src.abilities) out.push({ ref: { kind: 'ability', letter: a.letter }, label: a.name, group: 'Ability', tiles: a.tiles })
  for (const c of src.items.filter((i) => typeof i.action_verb === 'string')) out.push({ ref: { kind: 'item', slot: c.slot }, label: typeof c.name === 'string' ? c.name : c.letter, group: 'Item', tiles: itemTiles(c) })
  return out
}

/** A slot's ref, checked: what storage gives back is whatever was written there. */
export function slotRefFrom(v: unknown): SlotRef | null {
  if (!v || typeof v !== 'object' || !('kind' in v)) return null
  if (v.kind === 'fire') return { kind: 'fire' }
  if ((v.kind === 'spell' || v.kind === 'ability') && 'letter' in v && typeof v.letter === 'string') return { kind: v.kind, letter: v.letter }
  if (v.kind === 'item' && 'slot' in v && typeof v.slot === 'number') return { kind: 'item', slot: v.slot }
  return null
}

/** A remembered spell or ability, checked as `slotRefFrom` checks a ref. */
export function knownActionFrom(v: unknown): KnownAction | null {
  if (!v || typeof v !== 'object' || !('letter' in v) || !('name' in v) || typeof v.letter !== 'string' || typeof v.name !== 'string') return null
  const tiles = 'tiles' in v && Array.isArray(v.tiles) ? v.tiles.flatMap((t: unknown) => (t && typeof t === 'object' && 't' in t && typeof t.t === 'number' ? [{ t: t.t, tex: 'tex' in t && typeof t.tex === 'number' ? t.tex : undefined }] : [])) : []
  return { letter: v.letter, name: v.name, tiles }
}
