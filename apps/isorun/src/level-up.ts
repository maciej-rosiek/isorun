import { formattedStringToText, type GameMessage, type PlayerState } from '@isorun/webtiles'
import { h, clear } from './dom'
import { glyph } from './glyphs'
import type { Focusable } from './focus'
import type { PadKind } from './gamepad'

export type StatKey = 'str' | 'int' | 'dex'

export interface StatOption {
  hotkey: string
  send(): void
}

export interface LevelUpHooks {
  icon(name: string): HTMLElement | null
  continueMore(): void
}

interface Stat {
  key: StatKey
  hotkey: string
  name: string
  effect: string
  icon: string
}

const STATS: readonly Stat[] = [
  { key: 'str', hotkey: 'S', name: 'Strength', effect: 'Melee damage, and less armour encumbrance', icon: 'FIGHTING_ON' },
  { key: 'int', hotkey: 'I', name: 'Intelligence', effect: 'Spell power and casting success', icon: 'SPELLCASTING_ON' },
  { key: 'dex', hotkey: 'D', name: 'Dexterity', effect: 'Accuracy, evasion and stealth', icon: 'DODGING_ON' },
]

const REACHED_RE = /^You have reached level (\d+)/
const DRAMATIC_GAIN_RE = /^Your experience leads to a dramatic increase/
const BANNER_MS = 2500
const LEAVE_MS = 450
const LINGER_MS = 1800
const ORDINARY_STAT_GAIN = 2
const DRAMATIC_STAT_GAIN = 4
const RISE_RECENT_MS = 3000
const MORE_ANSWER_MS = 250

interface Snapshot {
  xl: number
  hpMax: number
  mpMax: number
  str: number
  int: number
  dex: number
}

function snapshot(p: PlayerState): Snapshot {
  return { xl: p.xl, hpMax: p.hp_max, mpMax: p.mp_max, str: p.str, int: p.int, dex: p.dex }
}

function unchanged(s: Snapshot, p: PlayerState): boolean {
  return s.xl === p.xl && s.hpMax === p.hp_max && s.mpMax === p.mp_max && s.str === p.str && s.int === p.int && s.dex === p.dex
}

export function reachedLevel(text: string): number | null {
  const m = REACHED_RE.exec(text)
  return m ? Number(m[1]) : null
}

function gains(from: Snapshot, to: Snapshot): { label: string; tone: string }[] {
  const out: { label: string; tone: string }[] = []
  const add = (delta: number, label: string, tone: string) => {
    if (delta > 0) out.push({ label: `+${delta} ${label}`, tone })
  }
  add(to.hpMax - from.hpMax, 'Max HP', 'hp')
  add(to.mpMax - from.mpMax, 'Max MP', 'mp')
  for (const s of STATS) add(to[s.key] - from[s.key], s.name, 'stat')
  return out
}

export class LevelUp {
  private el: HTMLElement | null = null
  private heading: HTMLElement | null = null
  private gainsEl: HTMLElement | null = null
  private choiceEl: HTMLElement | null = null
  private choiceFocus: Focusable[] = []
  private baseline: Snapshot | null = null
  private current: Snapshot | null = null
  private rise: { from: Snapshot; at: number } | null = null
  private statGain = ORDINARY_STAT_GAIN
  private gainsKey = ''
  private more = false
  private moreAnsweredAt = -Infinity
  private leaveTimer = 0
  private removeTimer = 0

  constructor(
    private readonly root: HTMLElement,
    private readonly hooks: LevelUpHooks,
  ) {}

  get choosing(): boolean {
    return this.choiceEl !== null
  }

  messages(lines: readonly GameMessage[]) {
    for (const line of lines) {
      const text = formattedStringToText(line.text).trim()
      const level = reachedLevel(text)
      if (level !== null) this.celebrate(level)
      if (DRAMATIC_GAIN_RE.test(text)) this.statGain = DRAMATIC_STAT_GAIN
    }
  }

  sync(player: PlayerState, more: boolean) {
    const before = this.current
    if (!before || !unchanged(before, player)) {
      this.current = snapshot(player)
      if (before && player.xl > before.xl) this.rise = { from: before, at: performance.now() }
    }
    const moreAnswered = this.more && !more
    if (moreAnswered) this.moreAnsweredAt = performance.now()
    this.more = more
    if (!this.el) return
    this.paintGains()
    if (moreAnswered && !this.choosing && !this.el.classList.contains('leaving')) this.linger()
  }

  choose(options: readonly StatOption[], pad: boolean, padKind: PadKind): Focusable[] {
    if (!this.el) this.open((this.current?.xl ?? 0) + 1)
    const el = this.el
    if (!el) return []
    this.stay()
    el.classList.add('choosing')
    el.classList.toggle('pad', pad)
    if (this.choiceEl) {
      this.paintFooter(pad, padKind)
      return this.choiceFocus
    }
    const cards = h('div', { class: 'level-up-cards' })
    this.choiceFocus = []
    STATS.forEach((stat, col) => {
      const option = options.find((o) => o.hotkey.toUpperCase() === stat.hotkey)
      if (!option) return
      const now = this.current?.[stat.key] ?? 0
      const card = h(
        'button',
        { type: 'button', class: 'level-up-card ' + stat.key, style: { '--i': String(col) } },
        h('div', { class: 'emblem' }, this.hooks.icon(stat.icon)),
        h('div', { class: 'name' }, stat.name),
        h('div', { class: 'value' }, h('span', { class: 'from' }, String(now)), h('span', { class: 'arrow' }, '→'), h('span', { class: 'to' }, String(now + this.statGain))),
        h('div', { class: 'effect' }, stat.effect),
        h('kbd', null, stat.hotkey),
      )
      card.addEventListener('click', option.send)
      cards.append(card)
      this.choiceFocus.push({ label: stat.name, el: card, activate: option.send, row: 0, col, id: 'stat:' + stat.hotkey })
    })
    this.choiceEl = h('div', { class: 'level-up-choice' }, h('div', { class: 'ask' }, `Raise an attribute by ${this.statGain}`), cards, h('div', { class: 'level-up-footer' }))
    el.querySelector('.level-up-panel')?.append(this.choiceEl)
    this.paintFooter(pad, padKind)
    return this.choiceFocus
  }

  endChoice() {
    if (!this.choiceEl) return
    this.choiceEl.remove()
    this.choiceEl = null
    this.choiceFocus = []
    this.statGain = ORDINARY_STAT_GAIN
    this.el?.classList.remove('choosing', 'pad')
    this.stay()
    this.leaveTimer = window.setTimeout(() => this.dismiss(), BANNER_MS)
  }

  destroy() {
    this.choiceEl = null
    this.close()
  }

  private celebrate(level: number) {
    if (this.el) {
      if (this.heading) this.heading.textContent = `Level ${level}`
      this.stay()
      if (!this.choosing) this.leaveTimer = window.setTimeout(() => this.dismiss(), BANNER_MS)
      return
    }
    this.open(level)
    this.leaveTimer = window.setTimeout(() => this.dismiss(), BANNER_MS)
  }

  private open(level: number) {
    const rise = this.rise && performance.now() - this.rise.at < RISE_RECENT_MS ? this.rise.from : null
    this.baseline = rise ?? this.current
    this.rise = null
    this.gainsKey = ''
    this.heading = h('div', { class: 'level' }, `Level ${level}`)
    this.gainsEl = h('div', { class: 'level-up-gains' })
    const panel = h(
      'div',
      { class: 'level-up-panel' },
      h('div', { class: 'level-up-rays' }),
      h('div', { class: 'level-up-glow' }),
      h('div', { class: 'kicker' }, 'You have reached'),
      this.heading,
      h('div', { class: 'rule' }),
      this.gainsEl,
    )
    panel.addEventListener('pointerdown', this.onPanelPointer)
    this.el = h('div', { class: 'level-up', 'data-client': '1' }, panel)
    this.root.append(this.el)
    window.addEventListener('keydown', this.onKey, true)
    window.addEventListener('pointerdown', this.onPointer, true)
    this.paintGains()
  }

  private paintGains() {
    const gainsEl = this.gainsEl
    const current = this.current
    if (!gainsEl || !current) return
    this.baseline ??= current
    const list = gains(this.baseline, current)
    const key = list.map((g) => g.label).join('|')
    if (key === this.gainsKey) return
    this.gainsKey = key
    clear(gainsEl)
    for (const g of list) gainsEl.append(h('span', { class: 'gain ' + g.tone }, g.label))
  }

  private paintFooter(pad: boolean, padKind: PadKind) {
    const footer = this.choiceEl?.querySelector('.level-up-footer')
    if (!footer) return
    clear(footer)
    if (pad) footer.append(glyph('DPAD', padKind), h('span', null, 'Pick'), glyph('A', padKind), h('span', null, 'Choose'))
    else footer.append(h('span', null, 'Press'), h('kbd', null, 'S'), h('kbd', null, 'I'), h('kbd', null, 'D'), h('span', null, 'or the arrows and'), h('kbd', null, 'Enter'))
  }

  private stay() {
    clearTimeout(this.leaveTimer)
    clearTimeout(this.removeTimer)
    this.el?.classList.remove('leaving')
  }

  private dismiss() {
    const el = this.el
    if (!el || this.choosing || el.classList.contains('leaving')) return
    clearTimeout(this.leaveTimer)
    el.classList.add('leaving')
    this.removeTimer = window.setTimeout(() => this.close(), LEAVE_MS)
  }

  private close() {
    this.stay()
    this.el?.remove()
    this.el = null
    this.heading = null
    this.gainsEl = null
    this.baseline = null
    window.removeEventListener('keydown', this.onKey, true)
    window.removeEventListener('pointerdown', this.onPointer, true)
  }

  private linger() {
    this.stay()
    this.leaveTimer = window.setTimeout(() => this.dismiss(), LINGER_MS)
  }

  private answer() {
    if (this.choosing) return
    if (this.more || performance.now() - this.moreAnsweredAt < MORE_ANSWER_MS) this.linger()
    else this.dismiss()
  }

  private onKey = (ev: KeyboardEvent) => {
    if (ev.key === 'Shift' || ev.key === 'Control' || ev.key === 'Alt' || ev.key === 'Meta') return
    this.answer()
  }

  private onPointer = (ev: PointerEvent) => {
    const panel = this.el?.firstElementChild
    if (panel && ev.target instanceof Node && panel.contains(ev.target)) return
    this.answer()
  }

  private onPanelPointer = (ev: PointerEvent) => {
    if (this.choosing) return
    ev.preventDefault()
    if (this.more) this.hooks.continueMore()
    this.answer()
  }
}
