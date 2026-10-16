import * as THREE from 'three/webgpu'

export type Point3 = [number, number, number]

export const SPRITE_LEAN = 0.3

/** Atlas texels to keep in from a tile's edges, so smooth filtering and mipmaps do not pick up the next tile in the atlas. */
export const ATLAS_INSET = 0.5

export class QuadBatch {
  readonly positions: number[] = []
  readonly uvs: number[] = []
  readonly colours: number[] = []
  readonly normals: number[] = []
  readonly indices: number[] = []

  /** Tips every normal toward up by this much (1 is 45 degrees for an upright quad), so upright sprites take some light from above; a light behind the sprite still leaves its front dark. */
  constructor(
    readonly atlasWidth: number,
    readonly atlasHeight: number,
    readonly lean = 0,
    readonly inset = 0,
  ) {}

  /** Corners top-left, top-right, bottom-right, bottom-left of the texel rect; `shade` is a grey level or a colour. */
  add(corners: [Point3, Point3, Point3, Point3], sx: number, sy: number, w: number, h: number, shade: number | Point3) {
    const base = this.positions.length / 3
    const insetX = Math.min(this.inset, w / 4)
    const insetY = Math.min(this.inset, h / 4)
    const u0 = (sx + insetX) / this.atlasWidth
    const u1 = (sx + w - insetX) / this.atlasWidth
    const v0 = (sy + insetY) / this.atlasHeight
    const v1 = (sy + h - insetY) / this.atlasHeight
    const uv = [u0, v0, u1, v0, u1, v1, u0, v1]
    for (const c of corners) this.positions.push(...c)
    const normal = outwardNormal(corners, this.lean)
    for (let i = 0; i < 4; i++) this.normals.push(...normal)
    this.uvs.push(...uv)
    const colour = typeof shade === 'number' ? [shade, shade, shade] : shade
    for (let i = 0; i < 4; i++) this.colours.push(...colour)
    this.indices.push(base, base + 2, base + 1, base, base + 3, base + 2)
  }

  geometry(): THREE.BufferGeometry {
    const g = new THREE.BufferGeometry()
    g.setAttribute('position', new THREE.Float32BufferAttribute(this.positions, 3))
    g.setAttribute('uv', new THREE.Float32BufferAttribute(this.uvs, 2))
    g.setAttribute('color', new THREE.Float32BufferAttribute(this.colours, 3))
    g.setAttribute('normal', new THREE.Float32BufferAttribute(this.normals, 3))
    g.setIndex(this.indices)
    return g
  }
}

/** The normal on the side the texture faces: down the left edge crossed with along the top edge. */
function outwardNormal([topLeft, topRight, , bottomLeft]: [Point3, Point3, Point3, Point3], lean: number): Point3 {
  const down = new THREE.Vector3(...bottomLeft).sub(new THREE.Vector3(...topLeft))
  const along = new THREE.Vector3(...topRight).sub(new THREE.Vector3(...topLeft))
  const n = down.cross(along).normalize()
  n.y += lean
  n.normalize()
  return [n.x, n.y, n.z]
}

export type BoxSide = 'top' | 'n' | 's' | 'w' | 'e'

const BOX_SIDES: Record<Exclude<BoxSide, 'top'>, { from: [number, number]; to: [number, number] }> = {
  n: { from: [1, 0], to: [0, 0] },
  s: { from: [0, 1], to: [1, 1] },
  w: { from: [0, 0], to: [0, 1] },
  e: { from: [1, 1], to: [1, 0] },
}

/**
 * A box from `min` to `max` (x, up, y) textured with one tile on every face
 * but the bottom. Each face takes as much of the tile as it is large, up to
 * the whole tile, so the texels keep their size on thin parts.
 */
export function addBox(batch: QuadBatch, rect: { sx: number; sy: number; w: number; h: number }, min: Point3, max: Point3, shade: number | Point3, sides: BoxSide[] = ['top', 'n', 's', 'w', 'e']) {
  const [x0, y0, z0] = min
  const [x1, y1, z1] = max
  const crop = (width: number, height: number): [number, number] => [rect.w * Math.min(1, width), rect.h * Math.min(1, height)]
  for (const side of sides) {
    if (side === 'top') {
      const [w, h] = crop(x1 - x0, z1 - z0)
      batch.add([[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]], rect.sx, rect.sy, w, h, shade)
      continue
    }
    const { from, to } = BOX_SIDES[side]
    const ax = from[0] ? x1 : x0
    const az = from[1] ? z1 : z0
    const bx = to[0] ? x1 : x0
    const bz = to[1] ? z1 : z0
    const [w, h] = crop(Math.hypot(bx - ax, bz - az), y1 - y0)
    batch.add([[ax, y1, az], [bx, y1, bz], [bx, y0, bz], [ax, y0, az]], rect.sx, rect.sy, w, h, shade)
  }
}
