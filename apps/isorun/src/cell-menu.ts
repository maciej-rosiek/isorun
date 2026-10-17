import { getCell, isThreat, type Scene } from '@isorun/scene'
import type { Action } from './bindings'
import { cellDetails, type CellDetail } from './cell-info'
import type { Context } from './context'
import { dockActions } from './dock-actions'
import { h } from './dom'
import { woundBar } from './hud'

/** What a row of the cell menu does: a click on the cell (1 left, 3 right, as `Runner.clickCell` numbers them), or one of the dock's actions. */
export type CellMenuRun = { kind: 'click'; button: 1 | 3 } | { kind: 'action'; action: Action }

export interface CellMenuRow {
  label: string
  run: CellMenuRun
}

/**
 * The rows of the menu a long press opens on a cell: on the player's own
 * square what it offers (the stairs, a pile to pick up: the dock's `here`
 * actions); elsewhere Attack for a hostile next to the player, Go here for
 * any other seen cell that is not a wall; and Describe, crawl's own
 * description (a right click), wherever there is something to describe.
 */
export function cellMenuRows(ctx: Context, scene: Scene, x: number, y: number): CellMenuRow[] {
  const cell = getCell(scene, x, y)
  if (!cell || cell.visibility === 'unseen') return []
  const rows: CellMenuRow[] = []
  const { x: px, y: py } = scene.player
  if (x === px && y === py) {
    for (const a of dockActions(ctx, scene)) if (a.here) rows.push({ label: a.label, run: { kind: 'action', action: a.action } })
  } else {
    const monster = cell.visibility === 'visible' ? scene.billboards.find((b) => b.x === x && b.y === y && b.kind === 'monster' && !b.scenery) : undefined
    const adjacent = Math.max(Math.abs(x - px), Math.abs(y - py)) === 1
    if (monster && isThreat(monster)) {
      if (adjacent) rows.push({ label: 'Attack', run: { kind: 'click', button: 1 } })
    } else if (cell.kind !== 'wall') rows.push({ label: 'Go here', run: { kind: 'click', button: 1 } })
  }
  if (cellDetails(scene, x, y).length) rows.push({ label: 'Describe', run: { kind: 'click', button: 3 } })
  return rows
}

/**
 * The cell menu on the screen: the cell's details (cell-info.ts, as the
 * inspect card shows them) over its rows, beside the point that was pressed
 * and kept inside `root`. `pick` runs a row; the owner closes it.
 */
export class CellMenu {
  readonly el: HTMLElement

  constructor(root: HTMLElement, details: readonly CellDetail[], rows: readonly CellMenuRow[], at: { x: number; y: number }, pick: (row: CellMenuRow) => void) {
    const head = details.flatMap((d) => {
      const line = h('div', { class: d.tone ? `line ${d.tone}` : 'line' }, d.text)
      return d.wound ? [line, woundBar(d.wound)] : [line]
    })
    const buttons = rows.map((row, i) => {
      const b = h('button', { type: 'button', class: 'inv-menu-row' + (i === 0 ? ' primary' : ''), role: 'menuitem' }, h('span', { class: 'label' }, row.label))
      b.addEventListener('click', (ev) => {
        ev.stopPropagation()
        pick(row)
      })
      return b
    })
    this.el = h('div', { class: 'cell-menu inspect', role: 'menu' }, head.length ? h('div', { class: 'cell-menu-title' }, ...head) : '', ...buttons)
    root.append(this.el)
    // beside the finger, not under it: above and to the right, turned back inside the screen where it would leave it
    const box = root.getBoundingClientRect()
    const w = this.el.offsetWidth
    const hgt = this.el.offsetHeight
    const gap = 24
    let left = at.x - box.left + gap
    if (left + w > box.width) left = at.x - box.left - gap - w
    let top = at.y - box.top - hgt - gap
    if (top < 0) top = at.y - box.top + gap
    this.el.style.left = Math.max(0, Math.min(left, box.width - w)) + 'px'
    this.el.style.top = Math.max(0, Math.min(top, box.height - hgt)) + 'px'
  }

  contains(node: Node | null): boolean {
    return !!node && this.el.contains(node)
  }

  remove() {
    this.el.remove()
  }
}
