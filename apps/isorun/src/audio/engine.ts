import type { Settings } from '../servers'
import { AMBIENT, AMBIENT_NAMES, MUSIC, SOUNDS, type AmbientName, type Mood, type SoundName, type SoundSpec } from './catalog'

interface Volumes {
  master: number
  effects: number
  ambient: number
  music: number
}

export interface PlayOptions {
  /** -1 is the left ear, 1 the right. */
  pan?: number
  /** Louder or softer than the sound is by itself. */
  gain?: number
  /** Seconds from now. */
  delay?: number
  rate?: number
}

/** The most sounds that play at once, of any kind. */
const MAX_VOICES = 24
/** Seconds a track takes to fade in or out, and the time a bed or a loop takes to follow the level it should have. */
const MUSIC_FADE = 3
const AMBIENT_FADE = 1.5
const VOLUME_FADE = 0.05

interface Stream {
  element: HTMLAudioElement
  source: MediaElementAudioSourceNode
  gain: GainNode
}

interface Layer {
  gain: GainNode
  stream: Stream | null
  source: AudioBufferSourceNode | null
  level: number
  stopTimer: number
}

/**
 * The game's sound: effects decoded once and played through small voices,
 * the ambient beds and the music on buses of their own, all under one master
 * gain that follows the settings. It starts on the first gesture of the page
 * (`unlock`), as browsers require, and sleeps while the tab is hidden. With
 * no `AudioContext` (a test, an old browser) every call does nothing.
 */
export class SoundEngine {
  /** When set, the name of every sound that is played is pushed on it. */
  trace: string[] | null = null
  /** Called when the music track that was playing has come to its end. */
  onMusicEnd: (() => void) | null = null
  private context: AudioContext | null = null
  private master: GainNode | null = null
  private effects: GainNode | null = null
  private ambient: GainNode | null = null
  private music: GainNode | null = null
  private readonly buffers = new Map<string, AudioBuffer>()
  private readonly loading = new Map<string, Promise<AudioBuffer | null>>()
  private readonly active = new Map<SoundName, number>()
  private readonly lastStart = new Map<SoundName, number>()
  private readonly layers = new Map<AmbientName, Layer>()
  private voices = 0
  private wantedMood: Mood | null = null
  private current: (Stream & { name: string }) | null = null
  private volumes: Volumes = { master: 0.7, effects: 1, ambient: 0.7, music: 0.5 }
  private ambientLevels: Partial<Record<AmbientName, number>> = {}

  constructor(private readonly base: string) {}

  get running(): boolean {
    return this.context?.state === 'running'
  }

  /** Start the sound, from a gesture of the player; does nothing the second time. */
  unlock(): void {
    if (this.context || typeof AudioContext === 'undefined') return
    const context = new AudioContext({ latencyHint: 'interactive' })
    this.context = context
    this.master = context.createGain()
    this.effects = context.createGain()
    this.ambient = context.createGain()
    this.music = context.createGain()
    this.effects.connect(this.master)
    this.ambient.connect(this.master)
    this.music.connect(this.master)
    this.master.connect(context.destination)
    this.applyVolumes()
    document.addEventListener('visibilitychange', this.onVisibility)
    void context.resume()
    void this.preload()
    this.setAmbient(this.ambientLevels)
    if (this.wantedMood) this.playMood(this.wantedMood)
  }

  /** Follow the volumes of the settings. */
  applySettings(settings: Pick<Settings, 'masterVolume' | 'effectsVolume' | 'ambientVolume' | 'musicVolume' | 'muted'>): void {
    this.volumes = {
      master: settings.muted ? 0 : settings.masterVolume / 100,
      effects: settings.effectsVolume / 100,
      ambient: settings.ambientVolume / 100,
      music: settings.musicVolume / 100,
    }
    this.applyVolumes()
  }

  /** Play an effect; a sound that is too frequent, too many at once, or not decoded yet is dropped. */
  play(name: SoundName, options: PlayOptions = {}): void {
    const context = this.context
    const effects = this.effects
    if (!context || !effects || context.state !== 'running') return
    const spec: SoundSpec = SOUNDS[name]
    const startAt = context.currentTime + (options.delay ?? 0)
    const last = this.lastStart.get(name) ?? -Infinity
    const count = this.active.get(name) ?? 0
    if (startAt - last < spec.cooldown || count >= spec.voices || this.voices >= MAX_VOICES) return
    const file = spec.files[Math.floor(Math.random() * spec.files.length)]
    const buffer = this.buffers.get(file)
    if (!buffer) return
    this.trace?.push(name)
    this.lastStart.set(name, startAt)
    const source = context.createBufferSource()
    source.buffer = buffer
    const base = spec.rate ?? 1
    source.playbackRate.value = base * (options.rate ?? 1) * (1 + (Math.random() * 2 - 1) * spec.pitchSpread)
    const gain = context.createGain()
    gain.gain.value = spec.gain * (options.gain ?? 1) * (1 - Math.random() * spec.gainSpread)
    source.connect(gain)
    const pan = options.pan ?? 0
    let tail: AudioNode = gain
    if (Math.abs(pan) > 0.02) {
      const panner = context.createStereoPanner()
      panner.pan.value = Math.max(-1, Math.min(1, pan))
      gain.connect(panner)
      tail = panner
    }
    tail.connect(effects)
    this.voices++
    this.active.set(name, count + 1)
    source.onended = () => {
      this.voices--
      this.active.set(name, (this.active.get(name) ?? 1) - 1)
      source.disconnect()
      tail.disconnect()
      gain.disconnect()
    }
    source.start(startAt)
  }

  /** Let the beds and loops sound at these levels (0 to 1); one not named fades out. */
  setAmbient(levels: Partial<Record<AmbientName, number>>): void {
    this.ambientLevels = levels
    const context = this.context
    if (!context) return
    for (const name of AMBIENT_NAMES) {
      const level = levels[name] ?? 0
      const layer = this.layers.get(name)
      if (!layer && level <= 0) continue
      this.layerFor(name, level)
    }
  }

  /** Fade to a random track of the mood, other than the one that plays. */
  playMood(mood: Mood): void {
    if (!this.context) {
      this.wantedMood = mood
      return
    }
    const tracks = MUSIC[mood].filter((track) => track !== this.current?.name)
    this.playTrack(tracks[Math.floor(Math.random() * tracks.length)])
  }

  /** Fade the music out. */
  stopMusic(): void {
    this.wantedMood = null
    this.fadeOut(this.current)
    this.current = null
  }

  /** Fade everything out and let go of the sound's hold on the page. */
  dispose(): void {
    this.stopMusic()
    this.setAmbient({})
  }

  private playTrack(name: string): void {
    const context = this.context
    const music = this.music
    if (!context || !music) return
    this.fadeOut(this.current)
    const element = new Audio(`${this.base}music/${name}.ogg`)
    element.preload = 'auto'
    const source = context.createMediaElementSource(element)
    const gain = context.createGain()
    gain.gain.value = 0
    source.connect(gain)
    gain.connect(music)
    gain.gain.setTargetAtTime(1, context.currentTime, MUSIC_FADE / 3)
    const track = { element, source, gain, name }
    this.current = track
    element.onended = () => {
      if (this.current !== track) return
      this.current = null
      this.fadeOut(track)
      this.onMusicEnd?.()
    }
    this.trace?.push(`music:${name}`)
    void element.play().catch(() => undefined)
  }

  private layerFor(name: AmbientName, level: number): void {
    const context = this.context
    const ambient = this.ambient
    if (!context || !ambient) return
    const spec = AMBIENT[name]
    let layer = this.layers.get(name)
    window.clearTimeout(layer?.stopTimer)
    if (!layer) {
      const gain = context.createGain()
      gain.gain.value = 0
      gain.connect(ambient)
      layer = { gain, stream: null, source: null, level: 0, stopTimer: 0 }
      this.layers.set(name, layer)
      this.startLayer(name, layer)
    }
    layer.level = level
    layer.gain.gain.setTargetAtTime(level * spec.gain, context.currentTime, AMBIENT_FADE / 3)
    if (level <= 0) layer.stopTimer = window.setTimeout(() => this.stopLayer(name), AMBIENT_FADE * 3000)
  }

  private startLayer(name: AmbientName, layer: Layer): void {
    const context = this.context
    if (!context) return
    const spec = AMBIENT[name]
    if (spec.stream) {
      const element = new Audio(`${this.base}ambient/${spec.file}.ogg`)
      element.loop = true
      const source = context.createMediaElementSource(element)
      source.connect(layer.gain)
      layer.stream = { element, source, gain: layer.gain }
      void element.play().catch(() => undefined)
      return
    }
    void this.load(`ambient/${spec.file}`).then((buffer) => {
      if (!buffer || this.layers.get(name) !== layer) return
      const source = context.createBufferSource()
      source.buffer = buffer
      source.loop = true
      source.connect(layer.gain)
      source.start()
      layer.source = source
    })
  }

  private stopLayer(name: AmbientName): void {
    const layer = this.layers.get(name)
    if (!layer || layer.level > 0) return
    this.layers.delete(name)
    layer.stream?.element.pause()
    layer.stream?.source.disconnect()
    layer.source?.stop()
    layer.source?.disconnect()
    layer.gain.disconnect()
  }

  private fadeOut(track: Stream | null): void {
    const context = this.context
    if (!track || !context) return
    track.gain.gain.setTargetAtTime(0, context.currentTime, MUSIC_FADE / 3)
    window.setTimeout(() => {
      track.element.pause()
      track.source.disconnect()
      track.gain.disconnect()
    }, MUSIC_FADE * 1000)
  }

  private applyVolumes(): void {
    const context = this.context
    if (!context || !this.master || !this.effects || !this.ambient || !this.music) return
    const { master, effects, ambient, music } = this.volumes
    for (const [node, value] of [[this.master, master], [this.effects, effects], [this.ambient, ambient], [this.music, music]] as const) node.gain.setTargetAtTime(value, context.currentTime, VOLUME_FADE)
  }

  private async preload(): Promise<void> {
    const files = new Set(Object.values(SOUNDS).flatMap((s) => s.files))
    await Promise.all([...files].map((file) => this.load(`sfx/${file}`).then((buffer) => buffer && this.buffers.set(file, buffer))))
  }

  private load(path: string): Promise<AudioBuffer | null> {
    const known = this.loading.get(path)
    if (known) return known
    const context = this.context
    const loaded = context
      ? fetch(`${this.base}${path}.ogg`)
          .then((r) => (r.ok ? r.arrayBuffer() : null))
          .then((data) => (data ? context.decodeAudioData(data) : null))
          .catch(() => null)
      : Promise.resolve(null)
    this.loading.set(path, loaded)
    return loaded
  }

  private onVisibility = () => {
    const context = this.context
    if (!context) return
    const streams = [this.current, ...[...this.layers.values()].map((l) => l.stream)]
    if (document.hidden) {
      void context.suspend()
      for (const stream of streams) stream?.element.pause()
    } else {
      void context.resume()
      for (const stream of streams) void stream?.element.play().catch(() => undefined)
    }
  }
}
