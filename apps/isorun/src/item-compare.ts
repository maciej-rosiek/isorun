import { formattedStringToText, type InvItem } from '@isorun/webtiles'
import { OBJ } from './gear'
import type { TipLine } from './tooltip'

type ComparableItem = Pick<InvItem, 'base_type' | 'sub_type' | 'name'>

const SHIELDS = new Set([12, 13, 14])

const ARMOUR_BASE: Readonly<Record<number, readonly [armourClass: number, encumbrance: number]>> = {
  0: [2, 0],
  1: [3, 4],
  2: [5, 7],
  3: [6, 10],
  4: [8, 14],
  5: [10, 18],
  6: [1, 0],
  7: [0, 0],
  8: [0, 0],
  9: [1, 0],
  10: [1, 0],
  11: [1, 0],
  12: [3, 5],
  13: [8, 10],
  14: [13, 15],
  15: [14, 23],
  16: [2, 0],
  18: [3, 4],
  20: [8, 9],
  22: [9, 11],
  24: [5, 0],
  26: [6, 5],
  28: [10, 15],
  30: [12, 23],
  32: [7, 7],
  34: [10, 11],
  35: [4, 6],
  36: [4, 6],
  38: [11, 15],
  40: [9, 7],
  41: [0, 0],
  42: [0, 0],
}

const WEAPON_BASE: Readonly<Record<number, readonly [damage: number, accuracy: number, delay: number]>> = {
  0: [5, 3, 13],
  1: [6, 2, 11],
  3: [8, 3, 14],
  4: [10, 0, 14],
  5: [13, -2, 15],
  7: [13, -3, 13],
  8: [15, -1, 15],
  9: [17, -4, 17],
  10: [4, 6, 10],
  11: [4, 6, 15],
  12: [5, 4, 10],
  13: [7, 4, 12],
  14: [8, 2, 13],
  15: [10, 1, 14],
  16: [12, 0, 14],
  17: [17, -3, 17],
  18: [7, 3, 13],
  19: [11, 0, 15],
  20: [13, -2, 16],
  21: [15, -4, 17],
  22: [18, -6, 19],
  23: [6, 4, 11],
  24: [9, 1, 13],
  25: [13, -3, 15],
  26: [15, -3, 17],
  27: [18, -6, 19],
  29: [16, -2, 19],
  30: [8, 2, 14],
  31: [14, 0, 17],
  32: [11, 1, 11],
  33: [20, -6, 16],
  34: [22, -7, 18],
  35: [13, -1, 13],
  36: [15, -1, 15],
  37: [19, -4, 18],
  38: [12, 1, 13],
  40: [5, 5, 12],
  41: [10, 3, 13],
  42: [16, -3, 14],
  43: [7, 0, 14],
  48: [14, -2, 12],
  51: [12, 0, 11],
  52: [13, 0, 13],
  54: [16, 3, 19],
  55: [23, -2, 23],
  57: [11, -3, 15],
  58: [14, 1, 17],
}

export interface ItemStat {
  label: string
  value: number
  higherIsBetter: boolean
  decimals: number
}

export interface ItemNameParts {
  enchant: number | null
  ego: string
  properties: string[]
}

export function itemNameParts(item: ComparableItem): ItemNameParts {
  const name = formattedStringToText(typeof item.name === 'string' ? item.name : '')
  const inscription = /\{([^}]*)\}/.exec(name)?.[1] ?? ''
  const bare = name.replace(/\{[^}]*\}/g, '').trim()
  const enchant = /(?:^|\s)([+-]\d+)\s/.exec(bare)?.[1]
  const ego = / of (.+?)\s*$/.exec(bare)?.[1] ?? ''
  return { enchant: enchant === undefined ? null : Number(enchant), ego, properties: inscription.split(/[\s,]+/).filter(Boolean) }
}

export function itemStats(item: ComparableItem): ItemStat[] {
  const { enchant } = itemNameParts(item)
  const plus = enchant ?? 0
  const sub = item.sub_type ?? -1
  if (item.base_type === OBJ.ARMOUR) {
    const base = ARMOUR_BASE[sub]
    if (!base) return []
    const [armourClass, encumbrance] = base
    const defence: ItemStat = SHIELDS.has(sub) ? { label: 'Shield', value: armourClass + plus, higherIsBetter: true, decimals: 0 } : { label: 'Armour', value: armourClass + plus, higherIsBetter: true, decimals: 0 }
    return encumbrance || SHIELDS.has(sub) ? [defence, { label: 'Encumbrance', value: encumbrance, higherIsBetter: false, decimals: 0 }] : [defence]
  }
  if (item.base_type === OBJ.WEAPONS) {
    const base = WEAPON_BASE[sub]
    if (!base) return []
    const [damage, accuracy, delay] = base
    return [
      { label: 'Damage', value: damage + plus, higherIsBetter: true, decimals: 0 },
      { label: 'Accuracy', value: accuracy + plus, higherIsBetter: true, decimals: 0 },
      { label: 'Attack delay', value: delay / 10, higherIsBetter: false, decimals: 1 },
    ]
  }
  if (item.base_type === OBJ.JEWELLERY && enchant !== null) return [{ label: 'Enchantment', value: enchant, higherIsBetter: true, decimals: 0 }]
  return []
}

export function compareStats(candidate: ComparableItem, worn: ComparableItem | undefined): TipLine[] {
  const theirs = worn ? itemStats(worn) : []
  return itemStats(candidate).map((stat) => {
    const other = theirs.find((t) => t.label === stat.label)
    const value = stat.value.toFixed(stat.decimals)
    if (!worn) return { label: stat.label, value }
    const diff = stat.value - (other?.value ?? 0)
    if (Math.abs(diff) < 1e-9) return { label: stat.label, value, tone: 'same' }
    const better = stat.higherIsBetter ? diff > 0 : diff < 0
    return { label: stat.label, value: `${value} (${diff > 0 ? '+' : ''}${diff.toFixed(stat.decimals)})`, tone: better ? 'better' : 'worse' }
  })
}
