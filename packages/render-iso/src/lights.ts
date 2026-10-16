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
  shading: { wrap: 0.25, bands: 5, relief: 0.6, sprites: 0.3 },
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
  lava: { enabled: true, crust: 0x1c0d09, molten: 0xb02a08, hot: 0xffa030, glow: 0.6, speed: 0.6 },
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
    l.shading = { wrap: 0.5, bands: 3, relief: 0.4, sprites: 0.3 }
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
} satisfies Record<string, IsoLights>

export type LightsName = keyof typeof LIGHTS

export function isLightsName(name: string): name is LightsName {
  return name in LIGHTS
}
