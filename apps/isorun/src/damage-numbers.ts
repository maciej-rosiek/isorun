import type { CombatEvent } from './combat'
import { h } from './dom'

type Cell = { x: number; y: number }

/** What floats up over an actor: its text, its colour (`tone`), how big it is and whether it shakes. */
interface Floater {
  text: string
  tone: 'dealt' | 'taken' | 'miss' | 'kill' | 'heal'
  scale: number
  shake: boolean
}

const LIFE_MS = 900
const MAX_ALIVE = 12
const DRIFT_PX = 22
const STACK_PX = 18
const STACK_MS = 350
/** How long a drop of the player's hit points waits for the blow that explains it. */
const CLAIM_MS = 1500
/** A blow that takes this share of the player's most hit points is big, and the next one very big. */
const HEAVY_SHARE = 0.2
const DEADLY_SHARE = 0.35
/** Hit points coming back at once, at least this many and this share of the most, read as healing. */
const HEAL_MIN = 5
const HEAL_SHARE = 0.15
const TIER_SCALES = [1, 1.15, 1.35, 1.7]
/** A blow whose line has no amount shows only from this many "!"s: weaker ones would be noise. */
const UNNUMBERED_TIER = 2
/** How long a blow on the player waits for the drop of the hit points that the server sends after its line. */
const DROP_WAIT_MS = 250

/** The damage a monster took: its amount, or crawl's "!"s for a strong blow when the line does not say; none for a weaker one. */
function dealtFloater(tier: number, amount: number | undefined): Floater | undefined {
  const strength = Math.min(tier, TIER_SCALES.length - 1)
  if (amount === 0) return { text: '0', tone: 'miss', scale: 0.9, shake: false }
  if (amount === undefined && strength < UNNUMBERED_TIER) return undefined
  return { text: amount === undefined ? '!'.repeat(strength) : String(amount), tone: 'dealt', scale: TIER_SCALES[strength], shake: strength === TIER_SCALES.length - 1 }
}

/** The damage the player took, as big as the share of the most hit points it took. */
function takenFloater(amount: number, hpMax: number): Floater {
  const share = hpMax > 0 ? amount / hpMax : 0
  const scale = share >= DEADLY_SHARE ? 2 : share >= HEAVY_SHARE ? 1.55 : share >= HEAVY_SHARE / 2 ? 1.2 : 1
  return { text: `-${amount}`, tone: 'taken', scale, shake: share >= HEAVY_SHARE }
}

/**
 * Numbers that pop over the actors as blows land (game.ts `trackCombat`):
 * the damage a monster took, the damage the player took sized by how much
 * of their life it was, misses, kills and healing. Each is a DOM element
 * over the canvas that follows its cell on the screen until it fades; the
 * frame loop runs only while one is up.
 */
export class DamageNumbers {
  private layer = h('div', { class: 'damage-numbers' })
  private alive: { el: HTMLElement; cell: Cell; lift: number; born: number }[] = []
  private frame = 0
  private hp: number | null = null
  private hpMax = 0
  private drops: { amount: number; t: number }[] = []

  constructor(
    host: HTMLElement,
    private screenOf: (x: number, y: number) => { x: number; y: number } | null,
    private enabled: () => boolean,
  ) {
    host.append(this.layer)
  }

  /**
   * The player's hit points as the server last said: a drop waits for the
   * blow that explains it (a hit on the player whose line has no amount, as
   * on a server's own crawl), and a big rise at once shows as healing.
   */
  health(hp: number, hpMax: number, at: Cell) {
    const now = performance.now()
    const was = this.hp
    const sameMax = hpMax === this.hpMax
    this.hp = hp
    this.hpMax = hpMax
    this.drops = this.drops.filter((d) => now - d.t < CLAIM_MS)
    if (was === null || !sameMax) return
    if (hp < was) this.drops.push({ amount: was - hp, t: now })
    else if (hp - was >= Math.max(HEAL_MIN, hpMax * HEAL_SHARE) && this.enabled()) this.spawn(at, { text: `+${hp - was}`, tone: 'heal', scale: 1.2, shake: false })
  }

  /** What to run when the blow of `e` lands on `at` (`IsoRender3d.act`'s `onShown`). */
  landing(e: CombatEvent, at: Cell | undefined): () => void {
    if (!at || e.kind === 'attack' || !this.enabled()) return () => {}
    if (e.kind !== 'hit') {
      if (e.kind === 'miss') return () => this.spawn(at, { text: 'miss', tone: 'miss', scale: 0.8, shake: false })
      return e.on === 'player' ? () => {} : () => this.spawn(at, { text: 'KILL', tone: 'kill', scale: 1.1, shake: false })
    }
    if (e.on !== 'player') {
      const dealt = dealtFloater(e.tier, e.amount)
      return () => dealt && this.spawn(at, dealt)
    }
    const { amount } = e
    const show = () => {
      const taken = this.claimDrops(amount)
      if (taken) this.spawn(at, takenFloater(taken, this.hpMax))
      return taken !== undefined
    }
    return () => {
      if (!show()) window.setTimeout(show, DROP_WAIT_MS)
    }
  }

  dispose() {
    cancelAnimationFrame(this.frame)
    this.frame = 0
    this.alive = []
    this.layer.remove()
  }

  /** The damage a blow on the player did: what its line says, taken off the drops waiting, or else all the drops waiting. */
  private claimDrops(said: number | undefined): number | undefined {
    const now = performance.now()
    const waiting = this.drops.filter((d) => now - d.t < CLAIM_MS).reduce((sum, d) => sum + d.amount, 0)
    const left = said === undefined ? 0 : Math.max(0, waiting - said)
    this.drops = left ? [{ amount: left, t: now }] : []
    return said ?? (waiting || undefined)
  }

  private spawn(cell: Cell, f: Floater) {
    const now = performance.now()
    const stacked = this.alive.filter((a) => a.cell.x === cell.x && a.cell.y === cell.y && now - a.born < STACK_MS).length
    const text = h('span', { class: `damage-text ${f.tone}`, style: { '--s': String(f.scale), '--dx': `${(Math.random() * 2 - 1) * DRIFT_PX}px` } }, f.text)
    const el = h('div', { class: 'damage-number' }, h('span', { class: f.shake ? 'damage-shake' : '' }, text))
    this.layer.append(el)
    this.alive.push({ el, cell: { x: cell.x, y: cell.y }, lift: stacked * STACK_PX, born: now })
    while (this.alive.length > MAX_ALIVE) this.alive.shift()?.el.remove()
    this.place(now)
    if (!this.frame) this.frame = requestAnimationFrame(this.tick)
  }

  private tick = () => {
    this.frame = 0
    this.place(performance.now())
    if (this.alive.length) this.frame = requestAnimationFrame(this.tick)
  }

  private place(now: number) {
    this.alive = this.alive.filter((a) => {
      const at = now - a.born < LIFE_MS ? this.screenOf(a.cell.x, a.cell.y) : null
      if (!at) a.el.remove()
      else a.el.style.transform = `translate(${at.x}px, ${at.y - a.lift}px)`
      return at !== null
    })
  }
}
