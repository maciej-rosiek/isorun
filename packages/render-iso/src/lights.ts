import type { ScreenLightSettings } from './post.js'
import type { ShadingSettings } from './shading.js'
import type { PaintSettings } from './enhance.js'
import type { FogSettings } from './fog.js'
import type { WaterSettings } from './water.js'
import type { LavaSettings } from './lava.js'
import type { CombatSettings } from './combat.js'
import type { StepSettings } from './steps.js'

export interface PointLightSpec {
  colour: number
  intensity: number
  /** Cells to where the light fades out. */
  distance: number
  decay: number
}

/** How the dungeon is lit and drawn: the lights, the shading, the tiles, the walls and the screen effects. */
export interface IsoLights {
  label: string
  exposure: number
  background: number
  ambient: { sky: number; ground: number; intensity: number }
  /** Daylight from the left of the screen, with shadows. */
  sun: { colour: number; intensity: number }
  torch: PointLightSpec
  /** Pixels of blur at the edge of shadows. */
  shadowSoftness: number
  shading: ShadingSettings
  paint: PaintSettings
  /** Walls: a sloped top edge (`bevel`, in cells) that catches the light, and moss on top. */
  walls: { bevel: number; moss: number; mossColour: number }
  ao: ScreenLightSettings['ao']
  outline: ScreenLightSettings['outline']
  painted: ScreenLightSettings['painted']
  pixelate: ScreenLightSettings['pixelate']
  bloom: ScreenLightSettings['bloom']
  tiltShift: ScreenLightSettings['tiltShift']
  grade: ScreenLightSettings['grade']
  /** The pointer on the ground: the colour of a move, and a brightness for every pointer colour. */
  aim: { colour: number; brightness: number }
  /** A drifting fog over the unexplored cells. */
  fog: FogSettings
  /** The drawn water: a rounded shore, shallow to deep colour, light lines and foam. */
  water: WaterSettings
  lava: LavaSettings
  /** What a hit looks like: the blood, the shake, the stop and the flash. */
  combat: CombatSettings
  /** The dust and the footprints of a step. */
  steps: StepSettings
}

const DIORAMA: IsoLights = {
  label: 'Diorama',
  exposure: 2.1,
  background: 0x212121,
  ambient: { sky: 0xe4ecff, ground: 0x8c7c6c, intensity: 2.25 },
  sun: { colour: 0xfff1dc, intensity: 4.85 },
  torch: { colour: 0xffb26a, intensity: 6, distance: 9.5, decay: 1 },
  shadowSoftness: 2.5,
  shading: { wrap: 0.25, bands: 5, relief: 0.3, depth: 0.06, sprites: 0.2, gloss: 0.25, rim: 0.4, cavity: 0.55, variation: 0.01, wetness: 0.25, glow: 0.9, sparkle: 0.65, dust: 0.5 },
  paint: { radius: 0, levels: 0, lift: 1.4, upscale: 'scale2x' },
  walls: { bevel: 0, moss: 0.25, mossColour: 0x707070 },
  ao: { enabled: true, intensity: 1, radius: 0.85 },
  outline: { enabled: false, thickness: 1, strength: 0.6, colour: 0x2a1e16 },
  painted: { enabled: false, radius: 0.5 },
  pixelate: { enabled: false, size: 3 },
  bloom: { enabled: false, strength: 0.5, radius: 0.4, threshold: 0.6 },
  tiltShift: { enabled: true, blur: 0.2, focus: 0.45, falloff: 0.5 },
  grade: { saturation: 0.95, shadows: 0xf0f1ff, highlights: 0xfef2e1, vignette: 0.2 },
  aim: { colour: 0xffd75e, brightness: 1 },
  fog: { enabled: true, colour: 0xa19b9b, density: 0.4, height: 1, reach: 2, softness: 0.05 },
  water: { enabled: true, deep: 0x0d1e26, shallow: 0x02061d, light: 0x54899c, strength: 0.15, speed: 0.7 },
  lava: { enabled: true, crust: 0x1c0d09, molten: 0xb02a08, hot: 0xffa030, glow: 0.4, speed: 0.6 },
  combat: { blood: 0x8a0f0f, gore: 1, shake: 0.4, hitStop: 0.07, flash: 1 },
  steps: { colour: 0x8a7a66, dust: 1, prints: 6 },
}

function variation(change: (lights: IsoLights) => void): IsoLights {
  const lights = structuredClone(DIORAMA)
  change(lights)
  return lights
}

export const LIGHTS = {
  diorama: DIORAMA,
  sunny: variation((l) => {
    l.label = 'Sunny clay'
    l.exposure = 1.8
    l.background = 0x3a3530
    l.ambient.intensity = 2.5
    l.sun = { colour: 0xfff4e0, intensity: 5.5 }
    l.shading = { ...l.shading, wrap: 0.5, bands: 3, relief: 0.4, sprites: 0.3, rim: 0.5, cavity: 0.45 }
    l.paint = { radius: 2, levels: 0, lift: 1.6, upscale: 'scale2x' }
    l.walls = { bevel: 0.1, moss: 0.25, mossColour: 0x8a8a7a }
    l.outline = { enabled: true, thickness: 1, strength: 0.5, colour: 0x2a1e16 }
    l.tiltShift.enabled = false
    l.grade = { saturation: 1.15, shadows: 0xf0f1ff, highlights: 0xfff4e4, vignette: 0.1 }
  }),
  storybook: variation((l) => {
    l.label = 'Storybook'
    l.background = 0x2a2420
    l.shading.bands = 4
    l.paint = { radius: 2, levels: 0, lift: 1.4, upscale: 'scale2x' }
    l.outline = { enabled: true, thickness: 1, strength: 0.7, colour: 0x2a1e16 }
    l.painted = { enabled: true, radius: 0.75 }
    l.grade = { saturation: 1.1, shadows: 0xe6e0ff, highlights: 0xffecd0, vignette: 0.25 }
  }),
  pixel: variation((l) => {
    l.label = 'Pixel'
    l.pixelate = { enabled: true, size: 3 }
    l.tiltShift.enabled = false
  }),
  dusk: variation((l) => {
    l.label = 'Dusk'
    l.exposure = 1.7
    l.background = 0x1a1418
    l.ambient = { sky: 0xa8b4e0, ground: 0x7a6858, intensity: 1.9 }
    l.sun = { colour: 0xffb070, intensity: 4 }
    l.torch.intensity = 8
    l.grade = { saturation: 0.95, shadows: 0xe0dcff, highlights: 0xffe6c8, vignette: 0.3 }
  }),
  moonlit: variation((l) => {
    l.label = 'Moonlit'
    l.exposure = 2
    l.background = 0x10141c
    l.shading.sprites = 0.6
    l.ambient = { sky: 0x8090c0, ground: 0x302838, intensity: 1.4 }
    l.sun = { colour: 0xa8c0ff, intensity: 2.5 }
    l.torch = { colour: 0xffa860, intensity: 9, distance: 8, decay: 1 }
    l.bloom = { enabled: true, strength: 0.4, radius: 0.4, threshold: 0.7 }
    l.grade = { saturation: 0.85, shadows: 0xc8d4ff, highlights: 0xfff2e0, vignette: 0.35 }
  }),
  rain: variation((l) => {
    l.label = 'Rain crypt'
    l.exposure = 1.9
    l.background = 0x0e1216
    l.ambient = { sky: 0x9fb4cc, ground: 0x3c4448, intensity: 1.7 }
    l.sun = { colour: 0xc8d8ec, intensity: 3 }
    l.torch = { colour: 0xffa858, intensity: 8, distance: 9, decay: 1 }
    l.shading = { ...l.shading, gloss: 0.7, rim: 0.5, cavity: 0.7, wetness: 0.9, dust: 0.2 }
    l.walls.mossColour = 0x4e5a4a
    l.water = { ...l.water, strength: 0.25, speed: 0.9 }
    l.grade = { saturation: 0.75, shadows: 0xd8e4ff, highlights: 0xfff0dc, vignette: 0.3 }
  }),
  ember: variation((l) => {
    l.label = 'Ember forge'
    l.exposure = 1.6
    l.background = 0x1c0e0a
    l.ambient = { sky: 0xb4a098, ground: 0x4a2a20, intensity: 1.3 }
    l.sun = { colour: 0xffa070, intensity: 2.4 }
    l.torch = { colour: 0xff9448, intensity: 7, distance: 9, decay: 1 }
    l.shading = { ...l.shading, gloss: 0.35, rim: 0.7, cavity: 0.7, glow: 1.6, dust: 1.4 }
    l.bloom = { enabled: true, strength: 0.3, radius: 0.5, threshold: 0.7 }
    l.lava = { ...l.lava, glow: 0.8 }
    l.grade = { saturation: 0.9, shadows: 0xe8dcff, highlights: 0xffe8c8, vignette: 0.4 }
  }),
  frost: variation((l) => {
    l.label = 'Frost'
    l.exposure = 2.1
    l.background = 0x161c24
    l.ambient = { sky: 0xd8ecff, ground: 0x6c7888, intensity: 2.4 }
    l.sun = { colour: 0xe4f0ff, intensity: 4.2 }
    l.torch = { colour: 0xffc890, intensity: 5, distance: 9, decay: 1 }
    l.shading = { ...l.shading, wrap: 0.35, gloss: 0.6, rim: 0.9, sparkle: 1.6, wetness: 0.4, dust: 0.3 }
    l.walls = { bevel: 0.05, moss: 0.35, mossColour: 0xdce8f4 }
    l.grade = { saturation: 0.7, shadows: 0xd4e4ff, highlights: 0xf4f8ff, vignette: 0.2 }
  }),
  miniature: variation((l) => {
    l.label = 'Miniature'
    l.exposure = 2
    l.background = 0x2c2620
    l.ambient.intensity = 2.5
    l.shading = { ...l.shading, wrap: 0.55, bands: 5, relief: 0.8, depth: 0.08, gloss: 0.15, rim: 0.3, cavity: 0.9, variation: 0.04 }
    l.tiltShift = { enabled: true, blur: 0.55, focus: 0.25, falloff: 0.35 }
    l.grade = { saturation: 1.25, shadows: 0xece4ff, highlights: 0xfff0dc, vignette: 0.15 }
  }),
  noir: variation((l) => {
    l.label = 'Noir'
    l.exposure = 1.6
    l.background = 0x080808
    l.ambient = { sky: 0xc0c0c0, ground: 0x303030, intensity: 0.9 }
    l.sun = { colour: 0xffffff, intensity: 4 }
    l.torch = { colour: 0xfff0e0, intensity: 7, distance: 7, decay: 1.4 }
    l.shading = { ...l.shading, bands: 3, gloss: 0.4, rim: 1.2, cavity: 0.8, glow: 0.6, dust: 0.7 }
    l.shadowSoftness = 1
    l.grade = { saturation: 0.12, shadows: 0xe8ecf4, highlights: 0xfff8ec, vignette: 0.6 }
  }),
} satisfies Record<string, IsoLights>

export type LightsName = keyof typeof LIGHTS

export function isLightsName(name: string): name is LightsName {
  return name in LIGHTS
}
