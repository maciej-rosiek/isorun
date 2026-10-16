import { formattedStringToText, type StatusEntry } from '@isorun/webtiles'
import type { Gamedata } from '@isorun/gamedata'
import { h, replace } from './dom'
import { drawTiles, type TileRef } from './overlays'
import { glyph } from './glyphs'
import type { Button, PadKind } from './gamepad'
import type { Action } from './bindings'
import type { DockAction, DockIcon } from './dock-actions'
import { SLOT_COUNT, type QuickSlot } from './quick-slots'
import { useKey } from './gear'
import { hudUnit } from './hud-unit'
import { keycaps, tipCard, type KeyHint, type Tip } from './tooltip'

/**
 * The dock: the HUD along the bottom of the screen, as an action RPG lays it
 * out. A health orb on the left and a magic orb on the right; between them
 * the quick slots, the experience bar, and the buttons that open every
 * screen (inventory, character, spells, abilities, skills, the level map,
 * the menu); over it the actions the square offers and the status lights.
 *
 * Everything is a button for the mouse, and the pad walks the same buttons
 * once L3 puts its cursor on the dock (`focusOn`). Nothing here is redrawn
 * on a frame: each part keeps the key it was last drawn for and is touched
 * only when that changes.
 */

export type DockButtonId = 'inventory' | 'character' | 'spells' | 'abilities' | 'skills' | 'map' | 'menu'

interface DockButtonDef {
  id: DockButtonId
  label: string
  about: string
  icon: string
  keys: readonly string[]
  /** the pad button that opens the same screen, worn as a badge while the pad is in use */
  pad?: Button
}

export const DOCK_BUTTONS: readonly DockButtonDef[] = [
  { id: 'inventory', label: 'Inventory', about: 'What you wear and carry', icon: 'CMD_DISPLAY_INVENTORY', keys: ['i'], pad: 'Y' },
  { id: 'character', label: 'Character', about: 'Your defences, attributes and status', icon: 'CMD_DISPLAY_CHARACTER_STATUS', keys: ['@'] },
  { id: 'spells', label: 'Spells', about: 'Cast a spell you know', icon: 'CMD_CAST_SPELL', keys: ['z'], pad: 'X' },
  { id: 'abilities', label: 'Abilities', about: 'Use an ability', icon: 'CMD_USE_ABILITY', keys: ['a'] },
  { id: 'skills', label: 'Skills', about: 'What you train, and how fast', icon: 'CMD_DISPLAY_SKILLS', keys: ['m'] },
  { id: 'map', label: 'Map', about: 'The level map, and travel', icon: 'CMD_DISPLAY_MAP', keys: ['X'], pad: 'SELECT' },
  { id: 'menu', label: 'Menu', about: 'Settings, help, save and exit', icon: 'CMD_GAME_MENU', keys: ['Esc'], pad: 'START' },
]

const ICON = { button: 40, action: 44, slot: 48 }

export interface DockHooks {
  onAction(a: Action): void
  onSlot(index: number): void
  /** pin something else on the slot: a right click, or Y on the pad */
  onSlotAssign(index: number): void
  onButton(id: DockButtonId): void
}

export interface DockView {
  hp: number
  hpMax: number
  /** the hit points poison will leave (`poison_survival`), drawn as a darker band in the orb */
  poisonSurvival: number
  mp: number
  mpMax: number
  xl: number
  progress: number
  gold: number
  place: string
  stats: DockStats
  statuses: readonly StatusEntry[]
  actions: readonly DockAction[]
  slots: readonly (QuickSlot | null)[]
  /** a spectator watches: the orbs and the bar only, nothing to press */
  spectating: boolean
  device: 'pad' | 'keyboard' | 'pointer'
  padKind: PadKind
}

export interface DockStats {
  ac: number
  ev: number
  sh: number
  acMod: number
  evMod: number
  shMod: number
  str: number
  int: number
  dex: number
  strMax: number
  intMax: number
  dexMax: number
}

/** One thing the pad's cursor stops on, on one of the dock's three rows. */
interface DockStop {
  el: HTMLElement
  row: number
  activate(): void
  assign?(): void
}

/** The 16 terminal colours, for the status lights (styles.css `--color-N` holds the rc's own). */
const statusColour = (col: number | undefined) => `var(--color-${(col ?? 7) & 15})`

export class Dock {
  root = h('div', { class: 'dock', hidden: true })
  private above = h('div', { class: 'dock-above' })
  private statusEl = h('div', { class: 'dock-status' })
  private actionsEl = h('div', { class: 'dock-actions' })
  private hpOrb = orb('hp', 'Health')
  private mpOrb = orb('mp', 'Magic')
  private slotsEl = h('div', { class: 'dock-slots' })
  /** the pad's way onto the dock, worn beside the slots while the pad is in use */
  private slotsPad = h('span', { class: 'slots-pad' })
  private xpFill = h('div', { class: 'xp-fill' })
  private xpLevel = h('span', { class: 'xp-level' })
  private xpEl = h('div', { class: 'dock-xp' })
  private statsEl = h('div', { class: 'dock-stats' })
  private buttonsEl = h('div', { class: 'dock-buttons' })
  private infoEl = h('div', { class: 'dock-info' })
  private tip = h('div', { class: 'dock-tip', hidden: true })
  private tips = new WeakMap<Element, Tip>()
  /** what the tooltip is over, so a change of what it says shows at once */
  private tipFor: HTMLElement | null = null
  private slotEls: { el: HTMLElement; canvas: HTMLCanvasElement; count: HTMLElement; key: HTMLElement; drawn: string }[] = []
  private buttonEls: HTMLElement[] = []
  private hooks: DockHooks
  private gd: Gamedata | null = null
  private keys = { orbs: '', status: '', actions: '', slots: '', xp: '', info: '', stats: '', mode: '', pad: '' }
  private actions: readonly DockAction[] = []
  private stops: DockStop[] = []
  private focus: number | null = null
  private unit = 0
  private padKind: PadKind = 'generic'
  private view: DockView | null = null

  constructor(hooks: DockHooks) {
    this.hooks = hooks
    this.xpEl.append(this.xpLevel, h('div', { class: 'xp-track' }, this.xpFill))
    this.slotsEl.append(this.slotsPad)
    this.setTip(this.slotsPad, { title: 'The dock on the pad', body: 'The hotbar, the actions and the screens', hints: [{ keys: [], pad: 'L3' }] })
    const mid = h('div', { class: 'dock-mid' }, this.slotsEl, this.xpEl, h('div', { class: 'dock-row' }, this.buttonsEl, this.statsEl, this.infoEl))
    this.above.append(this.statusEl, this.actionsEl)
    this.root.append(this.above, h('div', { class: 'dock-frame' }, this.hpOrb.el, mid, this.mpOrb.el), this.tip)
    this.setTip(this.hpOrb.el, { title: 'Health' })
    this.setTip(this.mpOrb.el, { title: 'Magic' })
    for (let i = 0; i < SLOT_COUNT; i++) {
      const canvas = h('canvas', { class: 'tile' })
      const count = h('span', { class: 'count' })
      const key = h('span', { class: 'slot-key' })
      const el = h('button', { type: 'button', class: 'slot empty' }, canvas, count, key)
      el.addEventListener('click', () => (el.classList.contains('empty') ? hooks.onSlotAssign(i) : hooks.onSlot(i)))
      el.addEventListener('contextmenu', (ev) => {
        ev.preventDefault()
        hooks.onSlotAssign(i)
      })
      this.slotEls.push({ el, canvas, count, key, drawn: '' })
      this.slotsEl.append(el)
    }
    for (const b of DOCK_BUTTONS) {
      const canvas = h('canvas', { class: 'tile' })
      const el = h('button', { type: 'button', class: 'dock-button ' + b.id, dataset: { icon: b.icon } }, canvas, h('span', { class: 'pad' }))
      this.setTip(el, { title: b.label, body: b.about, hints: [{ keys: b.keys, pad: b.pad }] })
      el.addEventListener('click', () => hooks.onButton(b.id))
      this.buttonEls.push(el)
      this.buttonsEl.append(el)
    }
    this.root.addEventListener('mouseover', (ev) => this.showTip(ev.target))
    this.root.addEventListener('mouseleave', () => (this.tip.hidden = true))
    this.root.addEventListener('pointerdown', (ev) => ev.stopPropagation())
    this.root.addEventListener('click', () => (this.tip.hidden = true))
    window.addEventListener('resize', () => this.rescale())
  }

  /** The dock's height in css px with the row of actions over it, for the layout to keep the message pane above both. */
  get height(): number {
    if (this.root.hidden) return 0
    const frame = this.root.querySelector('.dock-frame')?.getBoundingClientRect().height ?? 0
    return frame + this.above.getBoundingClientRect().height
  }

  setShown(shown: boolean) {
    if (this.root.hidden === !shown) return
    this.root.hidden = !shown
    if (!shown) this.focusOff()
  }

  setTiles(gd: Gamedata | null) {
    if (gd === this.gd) return
    this.gd = gd
    this.redrawIcons()
  }

  private px(size: number): number {
    return Math.round(size * this.unit)
  }

  private redrawIcons() {
    if (!this.unit) this.unit = hudUnit()
    for (const el of this.buttonEls) {
      const c = el.querySelector('canvas')
      if (c) drawIcon(c, this.gd, { gui: el.dataset.icon }, this.px(ICON.button))
    }
    for (const s of this.slotEls) s.drawn = ''
    this.keys.slots = ''
    this.keys.actions = ''
  }

  private rescale() {
    const unit = hudUnit()
    if (unit === this.unit) return
    this.unit = unit
    this.redrawIcons()
    if (this.view && !this.root.hidden) this.update(this.view)
  }

  update(v: DockView) {
    this.view = v
    if (this.root.hidden) return
    if (!this.unit) this.rescale()
    this.padKind = v.padKind
    const mode = v.spectating ? 'watch' : 'play'
    if (mode !== this.keys.mode) {
      this.keys.mode = mode
      this.root.classList.toggle('watching', v.spectating)
    }
    this.updateOrbs(v)
    this.updateStatus(v.statuses)
    this.updateActions(v.spectating ? [] : v.actions)
    this.updateSlots(v.slots, v.device === 'pad')
    const xp = v.xl + ':' + v.progress
    if (xp !== this.keys.xp) {
      this.keys.xp = xp
      this.xpFill.style.width = v.progress + '%'
      this.xpLevel.textContent = String(v.xl)
      this.setTip(this.xpEl, { kicker: 'Experience', title: `Level ${v.xl}`, body: `${v.progress}% of the way to level ${v.xl + 1}` })
    }
    const info = v.gold + '|' + v.place
    if (info !== this.keys.info) {
      this.keys.info = info
      const place = h('span', { class: 'place' }, v.place)
      const gold = h('span', { class: 'gold' }, h('span', { class: 'coin' }), String(v.gold))
      this.setTip(place, { title: v.place, body: 'Where you are' })
      this.setTip(gold, { title: `${v.gold} gold` })
      replace(this.infoEl, place, gold)
    }
    this.updateStats(v.stats)
    const pad = v.device === 'pad' ? v.padKind : ''
    if (pad !== this.keys.pad) {
      this.keys.pad = pad
      this.root.classList.toggle('pad', !!pad)
      DOCK_BUTTONS.forEach((b, i) => {
        const badge = this.buttonEls[i].querySelector('.pad')
        if (badge) replace(badge, pad && b.pad ? glyph(b.pad, v.padKind) : null)
      })
      replace(this.slotsPad, pad ? glyph('L3', v.padKind) : null)
    }
  }

  private setTip(el: Element, tip: Tip) {
    this.tips.set(el, tip)
    if (this.tipFor === el && !this.tip.hidden && el instanceof HTMLElement) this.placeTip(el)
  }

  private tipOf(el: Element): Tip | undefined {
    return this.tips.get(el)
  }

  private updateOrbs(v: DockView) {
    const key = [v.hp, v.hpMax, v.poisonSurvival, v.mp, v.mpMax].join(',')
    if (key === this.keys.orbs) return
    this.keys.orbs = key
    const share = (n: number, max: number) => (max > 0 ? Math.max(0, Math.min(1, n / max)) : 0)
    this.hpOrb.set(share(v.hp, v.hpMax), v.hp, v.hpMax, v.poisonSurvival < v.hp ? share(v.poisonSurvival, v.hpMax) : null)
    this.mpOrb.set(share(v.mp, v.mpMax), v.mp, v.mpMax, null)
    this.mpOrb.el.classList.toggle('none', v.mpMax <= 0)
    this.hpOrb.el.classList.toggle('low', v.hpMax > 0 && v.hp / v.hpMax <= 0.25)
    const poisoned = v.poisonSurvival < v.hp ? `Poison will leave you ${v.poisonSurvival}` : undefined
    this.setTip(this.hpOrb.el, { title: 'Health', sub: `${v.hp} of ${v.hpMax}`, body: poisoned })
    this.setTip(this.mpOrb.el, { title: 'Magic', sub: `${v.mp} of ${v.mpMax}` })
  }

  private updateStats(s: DockStats) {
    const key = JSON.stringify(s)
    if (key === this.keys.stats) return
    this.keys.stats = key
    replace(
      this.statsEl,
      this.defenceStat('AC', 'Armour class', s.ac, s.acMod, 'How much of a blow your armour takes'),
      this.defenceStat('EV', 'Evasion', s.ev, s.evMod, 'How often blows and missiles miss you'),
      this.defenceStat('SH', 'Shield', s.sh, s.shMod, 'How often your shield blocks'),
      h('i', { class: 'stat-gap' }),
      this.attributeStat('Str', 'Strength', s.str, s.strMax),
      this.attributeStat('Int', 'Intelligence', s.int, s.intMax),
      this.attributeStat('Dex', 'Dexterity', s.dex, s.dexMax),
    )
  }

  private defenceStat(label: string, name: string, value: number, change: number, about: string): HTMLElement {
    const changed = change > 0 ? ' raised' : change < 0 ? ' lowered' : ''
    const el = h('span', { class: 'stat defence' + changed }, h('b', null, String(value)), h('span', { class: 'k' }, label))
    this.setTip(el, { title: `${name} ${value}`, body: change ? `${about}\n${change > 0 ? '+' : ''}${change} for now` : about })
    return el
  }

  private attributeStat(label: string, name: string, value: number, max: number): HTMLElement {
    const drained = value < max
    const el = h('span', { class: 'stat attribute' + (drained ? ' lowered' : '') }, h('b', null, String(value)), h('span', { class: 'k' }, label))
    this.setTip(el, { title: `${name} ${value}`, body: drained ? `Drained: ${value} of ${max}` : undefined })
    return el
  }

  private updateStatus(statuses: readonly StatusEntry[]) {
    const lights = statuses.filter((s) => s.light)
    const key = JSON.stringify(lights)
    if (key === this.keys.status) return
    this.keys.status = key
    replace(this.statusEl, ...lights.map((s) => this.statusChip(s)))
    if (this.tipFor && !this.tipFor.isConnected) this.tip.hidden = true
  }

  private statusChip(s: StatusEntry): HTMLElement {
    const light = s.light ?? ''
    const about = (s.desc || s.text || '').trim()
    const el = h('span', { class: 'status-chip', style: { '--status': statusColour(s.col) } }, h('i', { class: 'status-gem' }), light)
    this.setTip(el, { title: light, colour: `color-mix(in oklab, ${statusColour(s.col)} 60%, #fff)`, body: about && about !== light ? about : undefined })
    return el
  }

  private updateActions(actions: readonly DockAction[]) {
    const key = JSON.stringify(actions.map((a) => [a.id, a.label, a.icon, a.here])) + !!this.gd + this.unit
    if (key === this.keys.actions) return
    this.keys.actions = key
    this.actions = actions
    replace(
      this.actionsEl,
      ...actions.map((a) => {
        const canvas = h('canvas', { class: 'tile' })
        drawIcon(canvas, this.gd, a.icon, this.px(ICON.action))
        const el = h(
          'button',
          { type: 'button', class: 'dock-action' + (a.here ? ' here' : '') },
          canvas,
          a.here ? h('span', { class: 'label' }, a.label) : null,
          a.key.keys.length ? h('span', { class: 'action-key' }, keycaps(a.key.keys)) : null,
        )
        this.setTip(el, { title: a.label, hints: [a.key] })
        el.addEventListener('click', () => this.hooks.onAction(a.action))
        return el
      }),
    )
    this.restops()
  }

  private updateSlots(slots: readonly (QuickSlot | null)[], pad: boolean) {
    const key = JSON.stringify(slots) + !!this.gd + pad + this.unit
    if (key === this.keys.slots) return
    this.keys.slots = key
    slots.forEach((slot, i) => {
      const s = this.slotEls[i]
      if (!s) return
      const k = JSON.stringify(slot) + pad
      if (k === s.drawn) return
      s.drawn = k
      s.el.classList.toggle('empty', !slot)
      s.el.classList.toggle('pinned', !!slot?.pinned)
      s.el.classList.toggle('useless', !!slot?.useless)
      s.el.classList.toggle('fire', slot?.ref.kind === 'fire')
      s.el.dataset.kind = slot?.ref.kind ?? ''
      const hint = slot ? slotHint(slot) : null
      replace(s.key, hint?.keys.length ? keycaps(hint.keys) : null)
      this.setTip(s.el, slot ? slotTip(slot, hint, pad) : { title: 'Empty slot', body: 'A spell, an ability or an item goes here', hints: [pad ? { keys: [], pad: 'A', label: 'Choose' } : { keys: ['Click'], label: 'Choose' }] })
      s.count.textContent = slot?.count && slot.count > 1 ? String(slot.count) : ''
      if (slot?.ref.kind === 'fire' && !slot.tiles.length) drawIcon(s.canvas, this.gd, { gui: 'CMD_AUTOFIGHT' }, this.px(ICON.slot))
      else drawLayers(s.canvas, this.gd, slot?.tiles ?? [], this.px(ICON.slot))
    })
  }

  private showTip(target: EventTarget | null) {
    let el = target instanceof Element ? target : null
    while (el && el !== this.root && !this.tips.has(el)) el = el.parentElement
    if (!el || el === this.root || !(el instanceof HTMLElement)) {
      this.tip.hidden = true
      return
    }
    this.placeTip(el)
  }

  private placeTip(el: HTMLElement) {
    const tip = this.tipOf(el)
    if (!tip) return
    this.tipFor = el
    replace(this.tip, tipCard(tip, this.padKind))
    this.tip.hidden = false
    const host = this.root.getBoundingClientRect()
    const r = el.getBoundingClientRect()
    const w = this.tip.offsetWidth
    const left = Math.max(-host.left + 4, Math.min(r.left - host.left + r.width / 2 - w / 2, window.innerWidth - host.left - w - 4))
    this.tip.style.left = left + 'px'
    this.tip.style.top = r.top - host.top - this.tip.offsetHeight - 10 + 'px'
  }

  /** Whether the pad's cursor is on the dock. */
  get focused(): boolean {
    return this.focus !== null
  }

  /** The pad's cursor onto the dock, on its first quick slot (or the first button with none). */
  focusOn() {
    this.restops()
    if (!this.stops.length) return
    const first = this.stops.findIndex((s) => s.row === 1 && !s.el.classList.contains('empty'))
    this.setFocus(first >= 0 ? first : this.stops.findIndex((s) => s.row === 2))
  }

  focusOff() {
    if (this.focus === null) return
    this.stops[this.focus]?.el.classList.remove('pad-focus')
    this.focus = null
    this.tip.hidden = true
    this.root.classList.remove('focused')
  }

  /** What the focused stop does on A and Y, for the prompts under the dock. */
  focusLabels(): { select: string; assign: string | null } | null {
    const s = this.focus === null ? null : this.stops[this.focus]
    if (!s) return null
    return { select: this.tipOf(s.el)?.title ?? 'Use', assign: s.assign ? 'Put something else here' : null }
  }

  /** A pad press while the cursor is on the dock; false when it is not. */
  nav(op: 'left' | 'right' | 'up' | 'down' | 'select' | 'assign' | 'cancel'): boolean {
    if (this.focus === null) return false
    const cur = this.stops[this.focus]
    if (!cur) return false
    if (op === 'cancel') {
      this.focusOff()
      return true
    }
    if (op === 'select') {
      cur.activate()
      return true
    }
    if (op === 'assign') {
      cur.assign?.()
      return true
    }
    if (op === 'left' || op === 'right') {
      const row = this.stops.map((s, i) => ({ s, i })).filter(({ s }) => s.row === cur.row)
      const at = row.findIndex(({ i }) => i === this.focus)
      const next = row[(at + (op === 'right' ? 1 : -1) + row.length) % row.length]
      if (next) this.setFocus(next.i)
      return true
    }
    const rows = [...new Set(this.stops.map((s) => s.row))].sort((a, b) => a - b)
    const ri = rows.indexOf(cur.row) + (op === 'down' ? 1 : -1)
    if (ri < 0 || ri >= rows.length) return true
    const x = centreX(cur.el)
    let best = -1
    this.stops.forEach((s, i) => {
      if (s.row !== rows[ri]) return
      if (best < 0 || Math.abs(centreX(s.el) - x) < Math.abs(centreX(this.stops[best].el) - x)) best = i
    })
    if (best >= 0) this.setFocus(best)
    return true
  }

  private setFocus(i: number) {
    if (this.focus !== null) this.stops[this.focus]?.el.classList.remove('pad-focus')
    this.focus = i
    const s = this.stops[i]
    this.root.classList.add('focused')
    if (!s) return
    s.el.classList.add('pad-focus')
    this.placeTip(s.el)
  }

  /** The stops, rebuilt when the actions change; the cursor keeps to the same row and place. */
  private restops() {
    const was = this.focus === null ? null : this.stops[this.focus]
    const stops: DockStop[] = []
    this.actionsEl.querySelectorAll<HTMLElement>('.dock-action').forEach((el, i) => stops.push({ el, row: 0, activate: () => this.hooks.onAction(this.actions[i].action) }))
    this.slotEls.forEach((s, i) =>
      stops.push({ el: s.el, row: 1, activate: () => (s.el.classList.contains('empty') ? this.hooks.onSlotAssign(i) : this.hooks.onSlot(i)), assign: () => this.hooks.onSlotAssign(i) }),
    )
    DOCK_BUTTONS.forEach((b, i) => stops.push({ el: this.buttonEls[i], row: 2, activate: () => this.hooks.onButton(b.id) }))
    this.stops = stops
    if (this.focus === null) return
    const same = was ? stops.findIndex((s) => s.el === was.el) : -1
    this.setFocus(same >= 0 ? same : stops.findIndex((s) => s.row === (was?.row ?? 1)))
  }
}

function centreX(el: HTMLElement): number {
  const r = el.getBoundingClientRect()
  return r.left + r.width / 2
}

const SLOT_KIND: Record<QuickSlot['ref']['kind'], string> = { spell: 'Spell', ability: 'Ability', item: 'Item', fire: 'Quiver' }

function slotHint(slot: QuickSlot): KeyHint {
  const ref = slot.ref
  if (ref.kind === 'spell') return { keys: ['z', ref.letter], label: 'Cast' }
  if (ref.kind === 'ability') return { keys: ['a', ref.letter], label: 'Use' }
  if (ref.kind === 'fire') return { keys: ['f'], pad: 'RB', label: 'Fire' }
  const key = useKey(slot.verb)
  return { keys: key && slot.letter ? [key, slot.letter] : [], label: slot.verb ?? 'Use' }
}

function slotTip(slot: QuickSlot, hint: KeyHint | null, pad: boolean): Tip {
  const hints: KeyHint[] = hint && (hint.keys.length || hint.pad) ? [hint] : []
  hints.push(pad ? { keys: [], pad: 'Y', label: 'Change' } : { keys: ['Right click'], label: 'Change' })
  return { kicker: SLOT_KIND[slot.ref.kind] + (slot.pinned ? ' · pinned' : ''), title: formattedStringToText(slot.label), body: slot.useless ? 'Of no use to you' : undefined, hints }
}

/** An orb: the liquid rises and falls with the value, the numbers over it. */
function orb(kind: 'hp' | 'mp', label: string) {
  const liquid = h('div', { class: 'orb-liquid' })
  const poison = h('div', { class: 'orb-poison' })
  const value = h('b')
  const max = h('span')
  const el = h('div', { class: 'orb ' + kind, 'aria-label': label }, h('div', { class: 'orb-well' }, liquid, poison, h('div', { class: 'orb-glass' })), h('div', { class: 'orb-text' }, value, h('i', null, '/'), max), h('div', { class: 'orb-rim' }))
  return {
    el,
    set(share: number, now: number, of: number, poisonShare: number | null) {
      liquid.style.setProperty('--fill', String(share))
      poison.hidden = poisonShare === null
      if (poisonShare !== null) poison.style.setProperty('--fill', String(poisonShare))
      value.textContent = String(now)
      max.textContent = String(of)
    },
  }
}

export function drawIcon(canvas: HTMLCanvasElement, gd: Gamedata | null, icon: DockIcon, size: number) {
  const id = icon.tile ?? (icon.gui ? gd?.gui.id(icon.gui) : icon.dngn ? gd?.dngn.id(icon.dngn) : undefined)
  drawLayers(canvas, gd, id === undefined ? [] : [{ t: id }], size)
}

export function drawLayers(canvas: HTMLCanvasElement, gd: Gamedata | null, tiles: readonly TileRef[], size: number) {
  canvas.style.width = size + 'px'
  canvas.style.height = size + 'px'
  drawTiles(canvas, gd, [...tiles], size)
}
