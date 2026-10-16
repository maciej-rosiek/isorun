import { formattedStringToText, indexToLetter, type InvItem, type MenuItem, type PlayerState } from '@isorun/webtiles'
import type { KeyOrText } from './bindings'

/**
 * The pack as the inventory screen draws it: what each item is worn as, and
 * where on the paper doll it hangs.
 *
 * The `player` message names the wielded weapon (`weapon_index`) and an
 * offhand weapon (`offhand_index`), and nothing else about what is worn
 * (tileweb.cc `_send_player`). Crawl's own pack says it after each name
 * (item-name.cc `DESC_INVENTORY_EQUIP`: "(weapon)", "(worn)", ...), so the
 * screen reads the rest off that menu (`wornFromMenu`).
 */

/** crawl's `object_class_type` (TAG_MAJOR_VERSION 34) */
export const OBJ = {
  WEAPONS: 0,
  MISSILES: 1,
  ARMOUR: 2,
  WANDS: 3,
  SCROLLS: 5,
  JEWELLERY: 6,
  POTIONS: 7,
  BOOKS: 8,
  STAVES: 9,
  MISCELLANY: 11,
  TALISMANS: 16,
  GIZMOS: 18,
  BAUBLES: 19,
} as const

/** jewellery.h: the amulets start here (`AMU_FIRST_AMULET`), the rings are below */
const FIRST_AMULET = 35

/** item-prop-enum.h `armour_type` under TAG_MAJOR_VERSION 34, by the slot each goes on; the rest are body armour */
const ARMOUR_SLOTS: Record<number, EquipSlot> = {
  6: 'cloak',
  7: 'helmet',
  8: 'helmet',
  9: 'helmet',
  10: 'gloves',
  11: 'boots',
  12: 'offhand',
  13: 'offhand',
  14: 'offhand',
  35: 'boots',
  36: 'boots',
  41: 'cloak',
  42: 'offhand',
}

export type EquipSlot = 'weapon' | 'offhand' | 'body' | 'cloak' | 'helmet' | 'gloves' | 'boots' | 'amulet' | 'ring' | 'talisman' | 'gizmo'

/** What crawl prints after an item's name in the pack (item-name.cc `DESC_INVENTORY_EQUIP`). */
export type Worn = 'weapon' | 'offhand' | 'worn' | 'installed' | 'melded' | 'quivered' | 'haunted'

export interface PackItem extends InvItem {
  /** the item's inventory letter, as crawl prints it: the gear's a-z and A-Z, the consumables' own after them */
  letter: string
}

const WORN_RE = /\((weapon|offhand|worn|installed|melded|quivered|haunted)\)/
const ROW_RE = /^\s*([a-zA-Z])\s*[-+#]\s/

/** The items carried, in pack order, each with its letter (tileweb.cc `_send_item` sends it as a character code). */
export function packItems(player: Pick<PlayerState, 'inv'>): PackItem[] {
  return Object.values(player.inv)
    .filter((it) => !!it && (it.quantity ?? 0) > 0 && it.base_type !== undefined)
    .sort((a, b) => a.slot - b.slot)
    .map((it) => ({ ...it, letter: typeof it.letter === 'number' ? String.fromCharCode(it.letter) : indexToLetter(it.slot) }))
}

/** What each letter of crawl's pack is worn as, read off the pack's rows ("a - a +0 war axe (weapon)"). */
export function wornFromMenu(items: readonly (MenuItem | undefined)[]): Map<string, Worn> {
  const out = new Map<string, Worn>()
  for (const item of items) {
    if (!item) continue
    const text = formattedStringToText(item.text)
    const row = ROW_RE.exec(text)
    const worn = WORN_RE.exec(text)
    if (row && worn && isWorn(worn[1])) out.set(row[1], worn[1])
  }
  return out
}

export function isWorn(v: string): v is Worn {
  return ['weapon', 'offhand', 'worn', 'installed', 'melded', 'quivered', 'haunted'].includes(v)
}

/** The doll slot an item goes on, or null for what is not worn (potions, scrolls, missiles). */
export function equipSlotOf(item: Pick<InvItem, 'base_type' | 'sub_type'>): EquipSlot | null {
  switch (item.base_type) {
    case OBJ.WEAPONS:
    case OBJ.STAVES:
      return 'weapon'
    case OBJ.ARMOUR:
      return ARMOUR_SLOTS[item.sub_type ?? -1] ?? 'body'
    case OBJ.JEWELLERY:
      return (item.sub_type ?? 0) >= FIRST_AMULET ? 'amulet' : 'ring'
    case OBJ.TALISMANS:
      return 'talisman'
    case OBJ.GIZMOS:
      return 'gizmo'
    default:
      return null
  }
}

/**
 * What is worn, by letter: the pack's own word where it was read (`known`),
 * and the `player` message's weapon and offhand weapon over it, which are
 * always current.
 */
export function wornNow(player: Pick<PlayerState, 'weapon_index' | 'offhand_index'>, items: readonly PackItem[], known: ReadonlyMap<string, Worn>): Map<string, Worn> {
  const out = new Map(known)
  for (const [letter, w] of out) if (w === 'weapon' || w === 'offhand') out.delete(letter)
  const weapon = items.find((i) => i.slot === player.weapon_index)
  const offhand = items.find((i) => i.slot === player.offhand_index)
  if (weapon) out.set(weapon.letter, 'weapon')
  if (offhand) out.set(offhand.letter, 'offhand')
  return out
}

/** The paper doll's places, in the order the pad walks them: the left column, the right column, the row under. */
export const DOLL_PLACES = ['helmet', 'cloak', 'body', 'gloves', 'boots', 'amulet', 'ring1', 'ring2', 'weapon', 'offhand', 'talisman', 'gizmo', 'quiver'] as const
export type DollPlace = (typeof DOLL_PLACES)[number]

/** The item on each place of the doll; the quiver is the quivered missile or launcher ammunition. */
export function paperDoll(items: readonly PackItem[], worn: ReadonlyMap<string, Worn>): Partial<Record<DollPlace, PackItem>> {
  const doll: Partial<Record<DollPlace, PackItem>> = {}
  for (const item of items) {
    const w = worn.get(item.letter)
    if (!w) continue
    if (w === 'quivered') {
      doll.quiver ??= item
      continue
    }
    if (w === 'offhand') {
      doll.offhand = item
      continue
    }
    const slot = equipSlotOf(item)
    if (!slot) continue
    if (slot === 'ring') {
      if (!doll.ring1) doll.ring1 = item
      else doll.ring2 ??= item
    } else doll[slot] ??= item
  }
  return doll
}

export type ItemActionId = 'equip' | 'unequip' | 'use' | 'drop' | 'describe'

export interface ItemAction {
  id: ItemActionId
  label: string
}

/** The verb crawl's own commands use for putting the item on, by its slot. */
function equipVerb(slot: EquipSlot): string {
  if (slot === 'weapon') return 'Wield'
  if (slot === 'amulet' || slot === 'ring') return 'Put on'
  return 'Wear'
}

function unequipVerb(slot: EquipSlot): string {
  if (slot === 'weapon') return 'Unwield'
  if (slot === 'amulet' || slot === 'ring') return 'Remove'
  return 'Take off'
}

/**
 * What can be done with an item from the inventory screen, the most likely
 * first: put it on or take it off, use it (the server's own verb for it,
 * `action_verb`: Quaff, Read, Evoke, Throw), drop it, read its description.
 */
export function itemActions(item: Pick<InvItem, 'base_type' | 'sub_type'> & { action_verb?: unknown }, worn: Worn | undefined): ItemAction[] {
  const out: ItemAction[] = []
  const slot = equipSlotOf(item)
  if (slot) {
    if (worn && worn !== 'quivered') out.push({ id: 'unequip', label: unequipVerb(slot) })
    else out.push({ id: 'equip', label: equipVerb(slot) })
  }
  const verb = typeof item.action_verb === 'string' ? item.action_verb : ''
  if (verb && verb.toLowerCase() !== equipVerb(slot ?? 'body').toLowerCase()) out.push({ id: 'use', label: sentenceCase(verb) })
  out.push({ id: 'drop', label: 'Drop' }, { id: 'describe', label: 'Describe' })
  return out
}

/**
 * The keys an action sends after the item's description is up (`inv_item_describe`): its own action line's key
 * (describe.cc `_get_action`), so crawl puts the item on or takes it off, or says why it cannot (a Minotaur's
 * helmet). Crawl's equip (`e`) and unequip (`c`) menus leave out what cannot be worn, and the letter would do
 * nothing there. A gizmo has no such key and goes through those menus. Drop is the description's `d` too, since
 * crawl's drop menu shows one page of the pack at a time and a letter there means a row of that page only.
 */
export function itemActionKeys(id: ItemActionId, item: Pick<PackItem, 'letter' | 'base_type' | 'sub_type'>): { describeFirst: boolean; seq: KeyOrText[] } | null {
  const slot = equipSlotOf(item)
  if (id === 'drop') return { describeFirst: true, seq: [{ text: 'd' }] }
  if (id !== 'equip' && id !== 'unequip') return null
  if (!slot || slot === 'gizmo') return { describeFirst: false, seq: [{ text: id === 'equip' ? 'e' : 'c' }, { text: item.letter, await: 'prompt' }] }
  return { describeFirst: true, seq: [{ text: DESCRIPTION_KEYS[slot][id] }] }
}

const DESCRIPTION_KEYS: Readonly<Record<Exclude<EquipSlot, 'gizmo'>, { equip: string; unequip: string }>> = {
  weapon: { equip: 'w', unequip: 'u' },
  offhand: { equip: 'w', unequip: 't' },
  body: { equip: 'w', unequip: 't' },
  cloak: { equip: 'w', unequip: 't' },
  helmet: { equip: 'w', unequip: 't' },
  gloves: { equip: 'w', unequip: 't' },
  boots: { equip: 'w', unequip: 't' },
  amulet: { equip: 'p', unequip: 'r' },
  ring: { equip: 'p', unequip: 'r' },
  talisman: { equip: 'p', unequip: 'r' },
}

const USE_KEYS: Readonly<Record<string, string>> = { quaff: 'q', drink: 'q', read: 'r', evoke: 'V', throw: 'F', fire: 'F' }

export function useKey(verb: unknown): string | undefined {
  return typeof verb === 'string' ? USE_KEYS[verb.toLowerCase()] : undefined
}

export function itemActionKeycaps(action: ItemAction, letter: string): string[] {
  if (action.id === 'equip') return ['e', letter]
  if (action.id === 'unequip') return ['c', letter]
  if (action.id === 'drop') return ['d', letter]
  if (action.id === 'describe') return ['i', letter]
  const key = useKey(action.label)
  return key ? [key, letter] : []
}

function sentenceCase(s: string): string {
  return s ? s[0].toUpperCase() + s.slice(1).toLowerCase() : s
}

/** The item's name without the article crawl puts first ("a +0 war axe" reads "+0 war axe" on a card). */
export function itemTitle(item: Pick<InvItem, 'name'>): string {
  const name = typeof item.name === 'string' ? item.name : ''
  return name.replace(/^(a|an|the|some)\s+/i, '')
}
