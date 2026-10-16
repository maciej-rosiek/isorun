import type { ActorStep } from '@isorun/render-iso'
import { getCell, type Scene } from '@isorun/scene'
import { formattedStringToText, MSGCH, type GameMessage } from '@isorun/webtiles'
import { monsterCell, type CombatEvent } from '../combat'
import type { Mood, SoundName } from './catalog'
import { ambientLevels, cuesForCombat, heartbeatDelay, hearing, moodFor, soundForLine, soundForUse, useKindOfKey, type Cell, type UseKind } from './cues'
import type { SoundEngine } from './engine'

/** What the director needs of the dungeon view. */
export interface AudioView {
  /** Where a cell is on the canvas, in CSS pixels. */
  screenOf(x: number, y: number): { x: number; y: number }
  width(): number
  animationSpeed(): number
}

/** How long a use of an item waits for the message that tells what came of it, in ms. */
const USE_WAIT_MS = 8000
/** After a new level the doors it builds are not opened by anyone, in ms. */
const LEVEL_SETTLE_MS = 800
/** A monster's footsteps fainter than this are not made. */
const FOOTSTEP_MIN_GAIN = 0.3
/** The quiet between two tracks, in ms. */
const MUSIC_PAUSE_MS = 6000
const MUSIC_PAUSE_SPREAD_MS = 14000
/** The start of the music after the first level is shown, in ms. */
const MUSIC_START_MS = 2500
/** Cells around the player that the water and the lava are counted in. */
const SURROUND = 4

/**
 * Turns what happens in a game into sounds: the fight, the steps, the doors,
 * the messages, the level and the player's health. It owns the timers of
 * the heartbeat and of the music's pauses, and the engine is the one place that
 * makes noise.
 */
export class GameAudio {
  private pending: { kind: UseKind; at: number } | null = null
  private place = ''
  private depth = -1
  private mood: Mood | null = null
  private levelChangedAt = -Infinity
  private heartbeat = 0
  private heartbeatDelay: number | undefined
  private musicTimer = 0
  private more = false
  private ambientKey = ''

  constructor(
    private readonly engine: SoundEngine,
    private readonly view: AudioView,
  ) {
    engine.onMusicEnd = () => {
      this.musicTimer = window.setTimeout(() => this.mood && engine.playMood(this.mood), MUSIC_PAUSE_MS + Math.random() * MUSIC_PAUSE_SPREAD_MS)
    }
  }

  /** The player's blow went out before the server answered. */
  swing(): void {
    this.engine.play('swing')
  }

  /** The sounds of a fight message, heard from where the fighters stand now; the returned call plays them, when the blow shows on the screen. */
  combat(event: CombatEvent, scene: Scene): () => void {
    const cellOf = (who: 'player' | number) => (who === 'player' ? scene.player : monsterCell(scene, who))
    const sounds = cuesForCombat(event, cellOf).flatMap((cue) => {
      const heard = this.heard(cue.at, scene.player)
      return heard ? [{ sound: cue.sound, pan: heard.pan, gain: heard.gain * (cue.gain ?? 1), delay: cue.after ?? 0 }] : []
    })
    return () => {
      for (const { sound, ...options } of sounds) this.engine.play(sound, options)
    }
  }

  /** An actor started a hop; its foot comes down near the end of it. */
  hop(step: ActorStep, scene: Scene): void {
    const seconds = step.seconds / this.view.animationSpeed()
    if (step.id === 'player') {
      this.engine.play('step', { delay: seconds * 0.8 })
      return
    }
    const heard = this.heard(step.to, scene.player)
    if (!heard || heard.gain < FOOTSTEP_MIN_GAIN) return
    this.engine.play('step', { pan: heard.pan, gain: 0.45 * heard.gain, delay: seconds * 0.8 })
  }

  door(cell: Cell, open: boolean, scene: Scene): void {
    if (performance.now() - this.levelChangedAt < LEVEL_SETTLE_MS) return
    const heard = this.heard(cell, scene.player)
    if (heard) this.engine.play(open ? 'door-open' : 'door-close', { pan: heard.pan, gain: heard.gain })
  }

  /** A message went to the server in the command mode: a key that uses an item waits for what came of it. */
  sent(message: { msg: string; text?: unknown; keycode?: unknown }, commandMode: boolean): void {
    if (message.msg === 'key' && message.keycode === 27) this.pending = null
    if (!commandMode || message.msg !== 'input' || typeof message.text !== 'string') return
    const kind = useKindOfKey(message.text)
    this.pending = kind ? { kind, at: performance.now() } : null
  }

  /** The message lines that arrived since the last ones. */
  messages(lines: readonly GameMessage[]): void {
    const texts = lines.filter((l) => l.channel !== MSGCH.PROMPT).map((l) => formattedStringToText(l.text).trim())
    if (!texts.length) return
    const pending = this.pending
    if (pending && performance.now() - pending.at < USE_WAIT_MS) {
      const sound = soundForUse(pending.kind, texts)
      if (sound) {
        this.engine.play(sound)
        this.pending = null
      }
    }
    const sounds = new Set<SoundName>()
    for (const text of texts) {
      const sound = soundForLine(text)
      if (sound) sounds.add(sound)
    }
    for (const sound of sounds) this.engine.play(sound)
  }

  /** The player's hit points, and whether the game waits at a --more--: the heart beats when they are low. */
  status(hp: number, hpMax: number, more: boolean, watching: boolean): void {
    if (more && !this.more) this.engine.play('more')
    this.more = more
    const delay = watching || hpMax <= 0 ? undefined : heartbeatDelay(hp / hpMax)
    this.heartbeatDelay = delay
    if (delay !== undefined && !this.heartbeat) this.beat()
  }

  /** The player is on this level; the music follows the place, the stairs sound when the level is a new one. */
  level(place: string, depth: number): void {
    if (place === this.place && depth === this.depth) return
    const first = this.depth < 0
    this.place = place
    this.depth = depth
    this.levelChangedAt = performance.now()
    this.ambientKey = ''
    if (!first) this.engine.play('stairs')
    const mood = moodFor(place, depth)
    if (mood === this.mood) return
    this.mood = mood
    window.clearTimeout(this.musicTimer)
    this.musicTimer = window.setTimeout(() => this.engine.playMood(mood), first ? MUSIC_START_MS : 0)
  }

  /** The player stands here: water and lava around add their sound to the bed of the place. */
  surroundings(scene: Scene): void {
    let water = 0
    let lava = 0
    const { x, y } = scene.player
    for (let dy = -SURROUND; dy <= SURROUND; dy++) {
      for (let dx = -SURROUND; dx <= SURROUND; dx++) {
        const flags = getCell(scene, x + dx, y + dy)?.flags
        if (flags?.water) water++
        if (flags?.lava) lava++
      }
    }
    const levels = ambientLevels(this.place, water, lava)
    const key = `${this.place}|${Math.round((levels.water ?? 0) * 5)}|${Math.round((levels.lava ?? 0) * 5)}`
    if (key === this.ambientKey) return
    this.ambientKey = key
    this.engine.setAmbient(levels)
  }

  dispose(): void {
    window.clearTimeout(this.heartbeat)
    window.clearTimeout(this.musicTimer)
    this.engine.onMusicEnd = null
    this.engine.dispose()
  }

  private beat(): void {
    const delay = this.heartbeatDelay
    if (delay === undefined) {
      this.heartbeat = 0
      return
    }
    this.engine.play('heartbeat')
    this.heartbeat = window.setTimeout(() => this.beat(), delay * 1000)
  }

  private heard(at: Cell | undefined, listener: Cell): { pan: number; gain: number } | undefined {
    if (!at) return { pan: 0, gain: 1 }
    const half = this.view.width() / 2
    return hearing(listener, at, (this.view.screenOf(at.x, at.y).x - half) / half)
  }
}
