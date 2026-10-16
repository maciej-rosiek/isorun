import { formattedStringToText } from '@isorun/webtiles'
import { NO_ACTION, barLabels, type Action, type BindingLabel } from './bindings'
import type { Context, Mode } from './context'
import { h, replace } from './dom'

const TOUCHED_BUTTONS: readonly BindingLabel['button'][] = ['B', 'LB', 'LT', 'X', 'Y', 'RT', 'RB', 'A']
const BARE_MODES: ReadonlySet<Mode> = new Set(['command', 'macro', 'text', 'yesno', 'prompt', 'lobby', 'spectating', 'newgame', 'dialog'])

export class TouchBar {
  readonly root = h('div', { class: 'touch-bar', hidden: true })
  private drawn = ''

  constructor(private run: (action: Action) => void) {}

  update(ctx: Context, shown: boolean) {
    const labels = shown && !BARE_MODES.has(ctx.mode) ? touchLabels(ctx) : []
    const key = labels.map((l) => l.button + ':' + l.label).join('|')
    if (key === this.drawn) return
    this.drawn = key
    this.root.hidden = labels.length === 0
    replace(this.root, ...labels.map((l) => this.button(l)))
  }

  private button(l: BindingLabel): HTMLElement {
    const role = l.button === 'A' ? ' primary' : l.button === 'B' ? ' cancel' : ''
    return h('button', { type: 'button', class: 'touch-button' + role, onclick: () => this.run(l.action) }, formattedStringToText(l.label))
  }
}

function touchLabels(ctx: Context): BindingLabel[] {
  const labels = barLabels(ctx).filter((l) => TOUCHED_BUTTONS.includes(l.button) && !l.teaching && l.label && l.label !== NO_ACTION)
  const sorted = labels.sort((a, b) => TOUCHED_BUTTONS.indexOf(b.button) - TOUCHED_BUTTONS.indexOf(a.button))
  return sorted.filter((l, i) => sorted.findIndex((other) => other.label === l.label) === i).reverse()
}
