/** One effect: the files it picks from at random, how loud and how varied it plays, and how many may sound at once. */
export interface SoundSpec {
  files: readonly string[]
  gain: number
  /** Playback rate of the file as it is, below 1 is lower and slower. */
  rate?: number
  /** How far the pitch may stray from `rate`, as a fraction. */
  pitchSpread: number
  /** How far the volume may fall below `gain`, as a fraction. */
  gainSpread: number
  voices: number
  /** Seconds the sound waits before it may sound again. */
  cooldown: number
}

const spec = (files: readonly string[], gain: number, voices: number, extra: Partial<SoundSpec> = {}): SoundSpec => ({ files, gain, voices, pitchSpread: 0.08, gainSpread: 0.15, cooldown: 0.03, ...extra })

/** Every effect by name; the files are in `public/audio/sfx`. */
export const SOUNDS = {
  step: spec(['step1', 'step2', 'step3', 'step4', 'step5'], 0.35, 3, { cooldown: 0.07, pitchSpread: 0.12 }),
  swing: spec(['swing1', 'swing2'], 0.6, 3),
  miss: spec(['miss1', 'miss2', 'miss3'], 0.7, 3),
  hit: spec(['hit1', 'hit2', 'hit3', 'hit4'], 0.8, 4),
  'hit-heavy': spec(['hit-heavy1', 'hit-heavy2'], 0.9, 2),
  hurt: spec(['hurt1', 'hurt2'], 1, 2, { cooldown: 0.1 }),
  death: spec(['death1', 'death2'], 0.9, 3, { rate: 0.75, pitchSpread: 0.15 }),
  heartbeat: spec(['heartbeat'], 0.9, 1, { pitchSpread: 0, gainSpread: 0, cooldown: 0.4 }),
  'door-open': spec(['door-open1', 'door-open2'], 0.7, 2, { cooldown: 0.15 }),
  'door-close': spec(['door-close1', 'door-close2'], 0.7, 2, { cooldown: 0.15 }),
  stairs: spec(['stairs'], 0.7, 1, { cooldown: 0.5 }),
  pickup: spec(['pickup1', 'pickup2'], 0.9, 2, { cooldown: 0.1 }),
  gold: spec(['gold1', 'gold2'], 1, 2, { cooldown: 0.1 }),
  levelup: spec(['levelup'], 2, 1, { pitchSpread: 0, gainSpread: 0 }),
  cast: spec(['cast-generic'], 1.4, 2),
  'cast-fire': spec(['cast-fire'], 0.7, 2),
  'cast-ice': spec(['cast-ice'], 0.8, 2, { rate: 0.8 }),
  'cast-dark': spec(['cast-dark'], 0.9, 2),
  potion: spec(['potion'], 1.4, 2),
  scroll: spec(['scroll1', 'scroll2'], 1.1, 2),
  'ui-click': spec(['ui-click1', 'ui-click2'], 0.5, 3, { pitchSpread: 0.04, cooldown: 0.04 }),
  'ui-open': spec(['ui-open'], 0.5, 2, { cooldown: 0.1 }),
  'ui-close': spec(['ui-close'], 0.5, 2, { cooldown: 0.1 }),
  'ui-back': spec(['ui-back'], 0.5, 2, { cooldown: 0.1 }),
  more: spec(['more'], 0.25, 1, { pitchSpread: 0, cooldown: 0.3 }),
  gameover: spec(['gameover'], 0.9, 1, { rate: 0.55, pitchSpread: 0 }),
} as const satisfies Record<string, SoundSpec>

export type SoundName = keyof typeof SOUNDS

/** The loops under the game: a bed of the dungeon, and the water or lava near the player. A stream is read as it plays, a buffer is decoded once. */
export const AMBIENT = {
  dungeon: { file: 'dungeon', gain: 0.9, stream: true },
  cavern: { file: 'cavern', gain: 0.8, stream: true },
  water: { file: 'water', gain: 0.9, stream: false },
  lava: { file: 'lava', gain: 0.8, stream: false },
} as const

export type AmbientName = keyof typeof AMBIENT
export const AMBIENT_NAMES: readonly AmbientName[] = ['dungeon', 'cavern', 'water', 'lava']

export type Mood = 'calm' | 'dark'

/** The tracks by mood; the files are in `public/audio/music`. */
export const MUSIC = {
  calm: ['ruined-temple', 'persistence', 'cave', 'abandoned-castle'],
  dark: ['evil-temple', 'eternal-sleep'],
} as const satisfies Record<Mood, readonly string[]>

export type MusicName = (typeof MUSIC)[Mood][number]
