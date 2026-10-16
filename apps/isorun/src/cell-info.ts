import { getCell, hasItemsUnder, type Billboard, type Scene, type WoundLevel } from '@isorun/scene'

/** One line of a cell's details; `tone` colours it (styles.css `.inspect`). */
export interface CellDetail {
  text: string
  tone?: Billboard['threat'] | 'friendly' | 'neutral' | 'note'
  wound?: WoundLevel
}

const WOUNDS: Partial<Record<WoundLevel, string>> = {
  lightly_damaged: 'lightly wounded',
  moderately_damaged: 'moderately wounded',
  heavily_damaged: 'heavily wounded',
  severely_damaged: 'severely wounded',
  almost_dead: 'almost dead',
}

/**
 * What a cell holds, in words, for the card about the cell the pointer or the
 * aim is on: a monster in sight with its threat, wounds and attitude, the
 * items that lie there (the top one of a pile, as the server names no more)
 * and the terrain (plain floor says nothing). Nothing for a cell never seen.
 */
export function cellDetails(scene: Scene, x: number, y: number): CellDetail[] {
  const cell = getCell(scene, x, y)
  if (!cell || cell.visibility === 'unseen') return []
  const here = scene.billboards.filter((b) => b.x === x && b.y === y)
  const details: CellDetail[] = []
  for (const b of here) {
    if (b.kind !== 'monster' || !b.name || cell.visibility !== 'visible') continue
    const friendly = b.attitude === 'friendly'
    const neutral = b.attitude === 'neutral' || b.attitude === 'good_neutral'
    details.push({ text: capitalise(b.name), tone: friendly ? 'friendly' : neutral ? 'neutral' : b.threat, wound: b.damage ?? 'uninjured' })
    const about = [friendly ? 'friendly' : neutral ? 'neutral' : b.threat && b.threat !== 'invisible' ? `${b.threat} threat` : '', b.damage ? (WOUNDS[b.damage] ?? '') : ''].filter(Boolean)
    if (about.length) details.push({ text: about.join(', '), tone: 'note' })
    if (hasItemsUnder(b)) details.push({ text: 'Items under it', tone: 'note' })
  }
  const items = here.filter((b) => b.kind === 'item' && b.name && !b.scenery)
  if (items.length) details.push({ text: capitalise(items.map((b) => b.name).join(', ')) + (items.some(hasItemsUnder) ? ' and more items' : '') })
  if (cell.label && cell.label !== 'floor') details.push({ text: capitalise(cell.label) + (cell.visibility === 'remembered' ? ' (remembered)' : ''), tone: 'note' })
  return details
}

function capitalise(text: string) {
  return text.charAt(0).toUpperCase() + text.slice(1)
}
