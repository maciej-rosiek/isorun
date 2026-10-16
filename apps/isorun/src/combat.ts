import { formattedStringToText, type GameMessage } from '@isorun/webtiles'
import type { Billboard, Scene } from '@isorun/scene'

/** Who acts in a fight: the player, or a monster by its client id (render-iso `ActorId`). */
export type Fighter = 'player' | number

/**
 * How hard a blow struck: `tier` is crawl's punctuation (attack.cc
 * `attack_strength_punctuation`), 0 for ".", 1 for "!", 2 for "!!", 3 for "!!!"
 * and more; `amount` is the damage done, when the line says it (" for 7",
 * engine/patches/11-damage-numbers.patch, so only on this device's games).
 */
export interface BlowStrength {
  tier: number
  amount?: number
}

/**
 * What a fight message shows: `attack` swings `by` at the cell `at`; `hit` and
 * `death` land on `on`, struck from the cell `from`; a hit says how hard.
 */
export type CombatEvent =
  | { kind: 'attack'; by: Fighter; at: { x: number; y: number } }
  | ({ kind: 'hit'; on: Fighter; from: { x: number; y: number } | undefined } & BlowStrength)
  | { kind: 'miss' | 'death'; on: Fighter; from: { x: number; y: number } | undefined }

/** "You see the goblin" and the like: verbs before a monster's name that are not blows. */
const NOT_BLOWS = new Set(['see', 'feel', 'sense', 'hear', 'notice', 'encounter', 'smell', 'open', 'close', 'are', 'have', 'can', 'cannot', 'swap', 'displace', 'stop', 'start', 'begin', 'finish', 'pick', 'drop', 'enter', 'leave', 'reach', 'reaches', 'cast', 'zap', 'throw', 'fire', 'shoot', 'evoke', 'pray', 'pacify', 'tame', 'charm'])
const KILL_VERBS = new Set(['kill', 'destroy', 'slay', 'annihilate', 'obliterate', 'dismember', 'demolish'])
const DEATH_ENDINGS = ['is killed', 'dies', 'is destroyed', 'is blown up', 'is incinerated', 'is annihilated', 'is obliterated', 'is dismembered', 'is slain', 'is consumed', 'is crushed', 'is turned to ash', 'disintegrates', 'falls apart', 'collapses', 'is drowned', 'is frozen solid']
/** How far a melee blow reaches, in king moves: two for a polearm. */
const REACH = 2

/**
 * The fight the message lines describe, in the order they were written:
 * the player's blows and kills ("You hit the goblin.", "You kill the
 * goblin!"), a monster's blows on the player ("The goblin hits you."), and
 * deaths and hits from anywhere ("The goblin is killed!", "The dart hits the
 * goblin."). A monster is found on the scene by the name the server gave it,
 * the nearest of that name to the player; a line that names no monster in
 * view shows nothing.
 */
export function combatEvents(lines: readonly GameMessage[], scene: Scene): CombatEvent[] {
  const monsters = scene.billboards.filter((b) => b.kind === 'monster' && monsterId(b) !== undefined)
  if (!monsters.length) return []
  const names = new Map<string, Billboard[]>()
  for (const b of monsters) {
    for (const name of monsterNames(b)) names.set(name, [...(names.get(name) ?? []), b])
  }
  const byLength = [...names.keys()].sort((a, b) => b.length - a.length)
  const player = scene.player
  const nearest = (name: string) => {
    const found = names.get(name) ?? []
    return found.reduce<Billboard | null>((best, b) => (best && kingMoves(best, player) <= kingMoves(b, player) ? best : b), null)
  }
  const events: CombatEvent[] = []
  for (const line of lines) {
    for (const sentence of sentences(formattedStringToText(line.text))) {
      const s = sentence.toLowerCase()
      if (/^the [\w\s-]+ (?:hits|strikes|burns|freezes|shocks|pierces|engulfs) you\b/.test(s) && !byLength.some((name) => s.startsWith(`the ${name} `))) {
        events.push({ kind: 'hit', on: 'player', from: undefined, ...blowStrength(s) })
        continue
      }
      for (const name of byLength) {
        const m = nearest(name)
        const id = m ? monsterId(m) : undefined
        if (!m || id === undefined || !s.includes(name)) continue
        const at = { x: m.x, y: m.y }
        const named = `(?:the |an? |your )?${escape(name)}`
        const blow = new RegExp(`^you (?:\\w+ly )?(\\w+) ${named}\\b`).exec(s)
        if (blow && !NOT_BLOWS.has(blow[1])) {
          if (kingMoves(m, player) <= REACH) events.push({ kind: 'attack', by: 'player', at })
          if (KILL_VERBS.has(blow[1])) events.push({ kind: 'death', on: id, from: player })
          else if (blow[1] === 'miss') events.push({ kind: 'miss', on: id, from: player })
          else events.push({ kind: 'hit', on: id, from: player, ...blowStrength(s) })
          break
        }
        const struck = new RegExp(`^${named} (?:\\w+ly )?(\\w+) you\\b`).exec(s)
        if (struck && !NOT_BLOWS.has(struck[1])) {
          events.push({ kind: 'attack', by: id, at: player })
          if (struck[1].startsWith('miss')) events.push({ kind: 'miss', on: 'player', from: at })
          else events.push({ kind: 'hit', on: 'player', from: at, ...blowStrength(s) })
          break
        }
        if (new RegExp(`^${named} \\w+s\\b.* at you\\b`).test(s)) {
          events.push({ kind: 'attack', by: id, at: player })
          break
        }
        if (DEATH_ENDINGS.some((ending) => new RegExp(`^${named} ${ending}\\b`).test(s))) {
          events.push({ kind: 'death', on: id, from: undefined })
          break
        }
        if (new RegExp(`\\b(?:hits|strikes|burns|freezes|shocks|pierces|bites|stings) ${named}\\b`).test(s) && !s.startsWith('you ')) {
          events.push({ kind: 'hit', on: id, from: undefined, ...blowStrength(s) })
          break
        }
      }
    }
  }
  return events
}

/** How hard the blow a sentence tells of struck: its "!"s, and the damage when it says " for 7". */
function blowStrength(sentence: string): BlowStrength {
  const tier = /!*$/.exec(sentence)?.[0].length ?? 0
  if (/\bno damage\b/.test(sentence)) return { tier, amount: 0 }
  const amount = / for (\d+)\b/.exec(sentence)?.[1]
  return amount === undefined ? { tier } : { tier, amount: Number(amount) }
}

/** Where the monster with this client id stands on the scene, if it is in view. */
export function monsterCell(scene: Scene, id: number): { x: number; y: number } | undefined {
  const found = scene.billboards.find((b) => b.kind === 'monster' && monsterId(b) === id)
  return found && { x: found.x, y: found.y }
}

/** The monster's client id, which the renderer knows its actor by. */
function monsterId(b: Billboard): number | undefined {
  const ref: unknown = b.ref
  if (typeof ref === 'object' && ref !== null && 'id' in ref && typeof ref.id === 'number') return ref.id
  return undefined
}

/** The names a message may call the monster by, lower case: its name and its plural. */
function monsterNames(b: Billboard): string[] {
  const ref: unknown = b.ref
  if (typeof ref !== 'object' || ref === null) return []
  const out: string[] = []
  if ('name' in ref && typeof ref.name === 'string' && ref.name) out.push(ref.name.toLowerCase())
  if ('plural' in ref && typeof ref.plural === 'string' && ref.plural) out.push(ref.plural.toLowerCase())
  return out
}

function kingMoves(a: { x: number; y: number }, b: { x: number; y: number }) {
  return Math.max(Math.abs(a.x - b.x), Math.abs(a.y - b.y))
}

function sentences(text: string): string[] {
  return text
    .split(/(?<=[.!?])\s+/)
    .map((s) => s.trim())
    .filter(Boolean)
}

function escape(s: string) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * The lines that arrived since `last` was the newest. When `last` is gone
 * (rolled back by the server, or nothing has been read yet) the lines of the
 * current `turn` stand in, so a fight is never lost and never older than
 * the turn it belongs to.
 */
export function linesSince(lines: readonly GameMessage[], last: GameMessage | undefined, turn: number): GameMessage[] {
  const i = last === undefined ? -1 : lines.lastIndexOf(last)
  if (i >= 0) return lines.slice(i + 1)
  return lines.filter((l) => l.turn === turn)
}
