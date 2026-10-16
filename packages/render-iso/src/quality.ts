/** How much work the renderer does for a frame: settings that change the speed more than the look. */
export interface IsoQuality {
  label: string
  /** The most device pixels the view draws per CSS pixel; a HiDPI screen draws fewer pixels and upscales them. */
  pixelRatio: number
  /** Ambient occlusion: the size of its image, a part of the screen's size; 0 turns it off. */
  aoResolution: number
  /** Whether lights cast shadows. */
  shadows: boolean
  /** How many of the wall torches nearest to the centre of the view cast shadows, 0 to 8. */
  torchShadows: number
  /** Whether the monsters cast torch shadows too, not only the sun's: each step of one draws a torch's shadow map again, six renders of the world. */
  actorTorchShadows: boolean
}

export const QUALITY = {
  low: { label: 'Low', pixelRatio: 1, aoResolution: 0, shadows: true, torchShadows: 0, actorTorchShadows: false },
  medium: { label: 'Medium', pixelRatio: 1.5, aoResolution: 0.5, shadows: true, torchShadows: 4, actorTorchShadows: false },
  high: { label: 'High', pixelRatio: 2, aoResolution: 1, shadows: true, torchShadows: 8, actorTorchShadows: true },
} satisfies Record<string, IsoQuality>

export type QualityName = keyof typeof QUALITY

export function isQualityName(name: string): name is QualityName {
  return name in QUALITY
}
