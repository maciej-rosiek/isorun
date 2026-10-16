import { QUALITY, SURFACE_DETAILS, type IsoQuality, type SurfaceDetail } from '@isorun/render-iso'
import { compactWindow, touchScreen } from './touch'
import type { Settings } from './servers'

/** The Resolution setting: the share of the screen's pixels the dungeon is drawn with, or the Quality preset's. */
export const RESOLUTIONS = ['auto', 100, 85, 75, 66, 50] as const
export type Resolution = (typeof RESOLUTIONS)[number]

/** The Ambient occlusion setting: off, or the soft shadows in corners at a size and a number of samples. */
export const AMBIENT_OCCLUSION = { off: { resolution: 0, samples: 8 }, low: { resolution: 0.5, samples: 8 }, medium: { resolution: 0.5, samples: 16 }, high: { resolution: 1, samples: 16 } } as const
export type AmbientOcclusion = 'auto' | keyof typeof AMBIENT_OCCLUSION

export const TORCH_SHADOWS = ['auto', 0, 2, 4, 8] as const
export type TorchShadows = (typeof TORCH_SHADOWS)[number]

/** What the renderer does a frame under these settings: the Quality preset, with each graphics setting that is not Auto in place of the preset's. */
export function qualityFor(s: Settings, devicePixelRatio: number): IsoQuality {
  const preset = QUALITY[s.graphics]
  const ao = s.ambientOcclusion === 'auto' ? null : AMBIENT_OCCLUSION[s.ambientOcclusion]
  return {
    ...preset,
    pixelRatio: s.resolution === 'auto' ? preset.pixelRatio : (devicePixelRatio * s.resolution) / 100,
    aoResolution: ao?.resolution ?? preset.aoResolution,
    aoSamples: ao?.samples ?? preset.aoSamples,
    antialias: s.antialias === 'auto' ? preset.antialias : s.antialias === 'on',
    torchShadows: s.torchShadows === 'auto' ? preset.torchShadows : s.torchShadows,
  }
}

/** Whether the screen is a phone's: the layout's compact window on a touch screen. */
function onPhone(): boolean {
  return touchScreen() && compactWindow()
}

/** The Surface detail levels offered here: a phone stops at Medium. */
export function surfaceDetails(): readonly SurfaceDetail[] {
  return onPhone() ? SURFACE_DETAILS.filter((level) => level !== 'high') : SURFACE_DETAILS
}

/** The Surface detail a player who never chose one gets: Off on a phone, Medium elsewhere. */
export function defaultSurfaceDetail(): SurfaceDetail {
  return onPhone() ? 'off' : 'medium'
}

