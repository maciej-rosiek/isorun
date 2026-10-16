import labelData from './tile-labels.json'

export const MATERIALS = ['stone', 'brick', 'rock', 'marble', 'dirt', 'sand', 'mud', 'grass', 'moss', 'wood', 'metal', 'gold', 'crystal', 'gem', 'ice', 'glass', 'bone', 'flesh', 'fur', 'scales', 'chitin', 'slime', 'fire', 'lava', 'water', 'cloth', 'leather', 'paper', 'plant', 'magic', 'shadow', 'light', 'other'] as const
export const JOINTS = ['none', 'bricks', 'blocks', 'cobbles', 'planks', 'tiles', 'cracks'] as const
export const MOTIONS = ['none', 'breathe', 'hover', 'flicker'] as const

export type Material = (typeof MATERIALS)[number]
export type Joints = (typeof JOINTS)[number]
export type Motion = (typeof MOTIONS)[number]

/** What a tile is and how its surface behaves, as tools/labels gave it (README there). */
export interface TileLabel {
  material: Material
  /** How bumpy, 0 (flat) to 3 (bricks, rock faces). */
  relief: number
  /** Whether the light texels are the raised parts. */
  brightIsHigh: boolean
  joints: Joints
  /** 0 matte to 3 very bright highlights. */
  gloss: number
  wet: boolean
  /** 0 none, 1 some parts give light, 2 all of it. */
  glow: number
  sparkle: boolean
  translucent: boolean
  motion: Motion
}

/** A tile no label names: a little relief from its brightness, matte, giving no light. */
export const UNLABELLED: TileLabel = { material: 'other', relief: 1, brightIsHigh: true, joints: 'none', gloss: 0, wet: false, glow: 0, sparkle: false, translucent: false, motion: 'none' }

function decode([material, relief, brightIsHigh, joints, gloss, wet, glow, sparkle, translucent, motion]: number[]): TileLabel {
  return {
    material: MATERIALS[material] ?? 'other',
    relief,
    brightIsHigh: brightIsHigh === 1,
    joints: JOINTS[joints] ?? 'none',
    gloss,
    wet: wet === 1,
    glow,
    sparkle: sparkle === 1,
    translucent: translucent === 1,
    motion: MOTIONS[motion] ?? 'none',
  }
}

const labels = labelData.labels.map(decode)
const byHash = new Map<string, TileLabel>(Object.entries(labelData.tiles).map(([hash, i]) => [hash, labels[i] ?? UNLABELLED]))

/** The label of a tile by its pixels (`tileHash`), or null when no label names that art. */
export function labelOf(hash: string): TileLabel | null {
  return byHash.get(hash) ?? null
}

/**
 * A tile's key for its labels: FNV-1a over its size and its pixels, with every texel under half alpha as nothing,
 * so the art finds its labels on any server that has it. tools/build/rltiles.mjs `tileHash` computes the same.
 */
export function tileHash(data: Uint8ClampedArray, width: number, height: number): string {
  let h = 0x811c9dc5
  const mix = (byte: number) => {
    h ^= byte
    h = Math.imul(h, 0x01000193) >>> 0
  }
  for (const v of [width & 255, width >> 8, height & 255, height >> 8]) mix(v)
  for (let i = 0; i < width * height * 4; i += 4) {
    const opaque = data[i + 3] >= 128
    mix(opaque ? data[i] : 0)
    mix(opaque ? data[i + 1] : 0)
    mix(opaque ? data[i + 2] : 0)
    mix(opaque ? 1 : 0)
  }
  return h.toString(16).padStart(8, '0')
}
