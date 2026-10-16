import { controlSheet, type SheetCell } from './bindings'
import { h } from './dom'
import type { PadKind } from './gamepad'
import { glyph, glyphName, type GlyphName } from './glyphs'

/**
 * The gamepad reference: what every button does, in the front end and the
 * in-game menu, as two tables side by side, one per half of the pad, each
 * read from the top of the controller down (they stack on a narrow screen).
 */
export function controlsSheet(padKind: PadKind = 'generic'): HTMLElement {
  // every direction is a step the way it points on the screen
  const lr = 'Move one step the way it points on the screen'
  const rs = 'Left / right turn the view; up / down zoom'
  const sticks: Partial<Record<GlyphName, [string, string]>> = { LSTICK: ['Left stick', lr], DPAD: ['D-pad', lr], RSTICK: ['Right stick', rs] }
  const buttons = new Map<GlyphName, SheetCell>(controlSheet().map((r) => [r.button, r.action]))
  const side = (order: readonly GlyphName[]) => {
    const table = h('table')
    for (const g of order) {
      const stick = sticks[g]
      const action = buttons.get(g)
      if (stick) table.append(h('tr', null, h('td', { class: 'key', title: stick[0] }, glyph(g, padKind)), h('td', null, stick[1])))
      else if (action)
        table.append(h('tr', null,
          h('td', { class: 'key', title: glyphName(g, padKind) }, glyph(g, padKind)),
          h('td', null, h('span', { class: 'tap' }, action.tap), action.hold ? h('span', { class: 'hold' }, glyph('HOLD', padKind), action.hold) : null)))
    }
    return table
  }
  const sides = h('div', { class: 'sides' },
    side(['LT', 'LB', 'LSTICK', 'L3', 'DPAD', 'SELECT']),
    side(['RT', 'RB', 'RSTICK', 'R3', 'A', 'B', 'X', 'Y', 'START']))
  return h('div', { class: 'body' }, sides,
    h('p', { class: 'hint' }, 'X opens actions as tabs (Spells, Abilities, Evocables, Quiver, Memorise); LB / RB turn them, Y swaps weapons and LT shouts. Select opens the level map, where LB / RB find stairs, Y travels to a branch, L3 finds items and R3 opens the overview. Y opens the inventory: with the HUD on Dock the paper doll and the backpack, where A lists what can be done with an item, Y does the first and X describes it; on Classic the pack, whose pages LB / RB turn. L3 puts the cursor on the dock: the quick slots, the actions and a button for every screen; A uses, Y changes a slot, B goes back. Start opens the Isorun menu, with your character on one tab and the game options on the other; LB / RB or left / right switches its tabs. Commands already on gamepad buttons are omitted. Up / down chooses a command; Page Up / Down pages. A / Start selects, B backs out, Select closes the menu outright. Browsing sends no game commands.'))
}
