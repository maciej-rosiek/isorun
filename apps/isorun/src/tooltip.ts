import { h, type Child } from './dom'
import { glyph, type GlyphName } from './glyphs'
import type { PadKind } from './gamepad'

export interface KeyHint {
  keys: readonly string[]
  pad?: GlyphName
  padHold?: boolean
  label?: string
}

export interface TipLine {
  label: string
  value: string
  tone?: 'better' | 'worse' | 'same'
}

export interface Tip {
  title: string
  colour?: string
  kicker?: string
  sub?: string
  body?: string
  lines?: readonly TipLine[]
  tags?: readonly { text: string; tone?: TipLine['tone'] }[]
  hints?: readonly KeyHint[]
}

export function keycaps(keys: readonly string[]): HTMLElement {
  return h('span', { class: 'keycaps kbd-only' }, ...keys.map((k) => h('kbd', { class: 'keycap' + (k.length > 1 ? ' wide' : '') }, k)))
}

export function tipCard(tip: Tip, padKind: PadKind, extraClass = ''): HTMLElement {
  const sections: Child[] = []
  if (tip.body) sections.push(h('div', { class: 'tip-body' }, tip.body))
  if (tip.lines?.length) sections.push(h('div', { class: 'tip-lines' }, ...tip.lines.map(tipLine)))
  if (tip.tags?.length) sections.push(h('div', { class: 'tip-tags' }, ...tip.tags.map((t) => h('span', { class: 'tip-tag' + (t.tone ? ' ' + t.tone : '') }, t.text))))
  return h(
    'div',
    { class: 'tip-card' + (extraClass ? ' ' + extraClass : '') },
    tip.kicker ? h('div', { class: 'tip-kicker' }, tip.kicker) : null,
    h('div', { class: 'tip-title', style: tip.colour ? { color: tip.colour } : null }, tip.title),
    tip.sub ? h('div', { class: 'tip-sub' }, tip.sub) : null,
    sections.length ? [h('div', { class: 'tip-rule' }), ...sections] : null,
    tip.hints?.length ? h('div', { class: 'tip-keys' }, ...tip.hints.map((hint) => keyHint(hint, padKind))) : null,
  )
}

function tipLine(line: TipLine): HTMLElement {
  return h('div', { class: 'tip-line' + (line.tone ? ' ' + line.tone : '') }, h('span', { class: 'k' }, line.label), h('span', { class: 'v' }, line.value))
}

function keyHint(hint: KeyHint, padKind: PadKind): HTMLElement {
  return h(
    'span',
    { class: 'tip-key' + (hint.pad ? '' : ' kbd-only') + (hint.keys.length ? '' : ' pad-only') },
    hint.keys.length ? keycaps(hint.keys) : null,
    hint.pad ? h('span', { class: 'tip-pad pad-only' }, glyph(hint.pad, padKind), hint.padHold ? h('span', { class: 'tip-hold' }, 'hold') : null) : null,
    hint.label ? h('span', { class: 'tip-key-label' }, hint.label) : null,
  )
}
