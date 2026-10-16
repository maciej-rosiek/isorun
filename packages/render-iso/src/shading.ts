import * as THREE from 'three/webgpu'
import { MEMORY_SHADE, type LiquidKind, type Scene } from '@isorun/scene'
import { Fn, If, Loop, attribute, clamp, diffuseColor, dFdx, dFdy, float, frontFacing, hash, int, interleavedGradientNoise, max, mix, mx_noise_float, normalMap, normalView, normalViewGeometry, normalWorld, normalWorldGeometry, positionLocal, positionView, positionViewDirection, positionWorld, screenCoordinate, select, shininess, smoothstep, specularColor, texture, time, uniform, uv, vec2, vec3, vec4 } from 'three/tsl'
import { emissionOutput } from './post.js'
import { UV_RECT } from './quads.js'

export interface ShadingSettings {
  /** How far the light wraps around a surface: 0 is plain diffuse light, 1 lights even surfaces that face away (a soft, clay look). */
  wrap: number
  /** Flat tones of light; 0 for a smooth change. */
  bands: number
  /** Strength of the bumps from the tile normal maps. */
  relief: number
  /** How deep the low parts of the tiles sink (parallax), in tiles; 0 keeps them flat. */
  depth: number
  /** Light on sprites (monsters, items): below 1 keeps the detail of bright sprites in strong light. */
  sprites: number
  /** Strength of the highlights on glossy surfaces (metal, wet stone, ice); 0 for none. */
  gloss: number
  /** A light edge where a surface turns away from the view and a light is behind it; 0 for none. */
  rim: number
  /** Dirt in the low parts of the tiles and wear on their edges; 0 for none. */
  cavity: number
  /** How much each cell's colour differs from the next, so repeated tiles do not read as a grid; 0 for none. */
  variation: number
  /** How wet the ground near water gets: darker, glossy, with puddles in its low parts; 0 for dry. */
  wetness: number
  /** Light given by runes, eyes, flames and other glowing parts; 0 for none. */
  glow: number
  /** Glints on crystal, gems and ice; 0 for none. */
  sparkle: number
  /** Dust drifting in the light of the torches; 0 for none. */
  dust: number
}

const wrap = uniform(0)
const bands = uniform(0)
const relief = uniform(1)
const depth = uniform(0)
/** Steps of the parallax into each tile: more is finer and costs more; 0 leaves the tiles flat. */
const depthSteps = uniform(16)
const spriteLight = uniform(1)
const glossStrength = uniform(0)
const rimStrength = uniform(0)
const cavityStrength = uniform(0)
const variationStrength = uniform(0)
const wetStrength = uniform(0)
const glowStrength = uniform(0)
const sparkleStrength = uniform(0)
/** How much dust shows in the torches' light (`dust.ts`). */
export const dustStrength = uniform(0)

export const SURFACE_DETAILS = ['off', 'low', 'medium', 'high'] as const
/** How much of the surface work is drawn (a graphics setting apart from the look): `off` is the look before it. */
export type SurfaceDetail = (typeof SURFACE_DETAILS)[number]

/** What each level of surface detail draws: the parallax's steps, and which effects of the look are on. */
const DETAIL: Record<SurfaceDetail, { depthSteps: number; gloss: boolean; rim: boolean; cavity: boolean; variation: boolean; wetness: boolean; glow: boolean; sparkle: boolean; dust: boolean; statuses: boolean; liquids: boolean }> = {
  off: { depthSteps: 0, gloss: false, rim: false, cavity: false, variation: false, wetness: false, glow: false, sparkle: false, dust: false, statuses: false, liquids: false },
  low: { depthSteps: 0, gloss: false, rim: true, cavity: true, variation: true, wetness: false, glow: false, sparkle: false, dust: false, statuses: true, liquids: false },
  medium: { depthSteps: 8, gloss: true, rim: true, cavity: true, variation: true, wetness: true, glow: true, sparkle: false, dust: false, statuses: true, liquids: false },
  high: { depthSteps: 16, gloss: true, rim: true, cavity: true, variation: true, wetness: true, glow: true, sparkle: true, dust: true, statuses: true, liquids: true },
}

let detail: SurfaceDetail = 'high'
let shading: ShadingSettings | null = null
/** 1 where the liquids have depth (the ground under water, ripples, starwater's layers, the lava's crust), 0 for flat liquids. */
export const liquidDepth = uniform(1)

/** Draw this much of the surface work; the look's strengths stay as they are, so a level back on brings them back. */
export function applySurfaceDetail(level: SurfaceDetail) {
  detail = level
  refreshShading()
}

/** Whether glowing tiles give light that the screen blooms, at this surface detail. */
export function glowShown(): boolean {
  return DETAIL[detail].glow && (shading?.glow ?? 0) > 0
}

export function applyShading(settings: ShadingSettings) {
  shading = settings
  refreshShading()
}

function refreshShading() {
  const on = DETAIL[detail]
  liquidDepth.value = on.liquids ? 1 : 0
  depthSteps.value = on.depthSteps
  if (!shading) return
  const gate = (shown: boolean, value: number) => (shown ? value : 0)
  wrap.value = shading.wrap
  bands.value = shading.bands
  relief.value = shading.relief
  depth.value = shading.depth
  spriteLight.value = shading.sprites
  glossStrength.value = gate(on.gloss, shading.gloss)
  rimStrength.value = gate(on.rim, shading.rim)
  cavityStrength.value = gate(on.cavity, shading.cavity)
  variationStrength.value = gate(on.variation, shading.variation)
  wetStrength.value = gate(on.wetness, shading.wetness)
  glowStrength.value = gate(on.glow, shading.glow)
  sparkleStrength.value = gate(on.sparkle, shading.sparkle)
  dustStrength.value = gate(on.dust, shading.dust)
  statusesShown = on.statuses
}

type DirectLight = THREE.LightingModelDirectInput & {
  lightDirection: THREE.Node<'vec3'>
  lightColor: THREE.Node<'vec3'>
  reflectedLight: { directDiffuse: THREE.Node<'vec3'>; directSpecular: THREE.Node<'vec3'> }
}

/** The narrowest highlight, of the glossiest surface, as a Blinn-Phong exponent; a matte one has the widest. */
const SHININESS = { matte: 8, glossy: 160 }
const RIM_POWER = 3
/** How much of the rim light a light at the side gives, against one right behind the surface. */
const RIM_SIDE = 0.35

/**
 * Diffuse light that wraps around surfaces and can step into flat tones, a Blinn-Phong highlight as strong as the
 * surface is glossy (`specularColor`, `shininess`), and a rim of light where the surface turns away from the view and
 * the light comes from behind it, as much as `rim` asks.
 */
class ClayLightingModel extends THREE.PhongLightingModel {
  constructor(private readonly rim: THREE.Node<'float'>) {
    super(false)
  }

  direct({ lightDirection, lightColor, reflectedLight }: DirectLight) {
    const wrapped = normalView.dot(lightDirection).add(wrap).div(wrap.add(1)).clamp()
    const banded = wrapped.mul(bands).ceil().div(bands.max(1))
    const shade = select(bands.greaterThan(0.5), banded, wrapped)
    reflectedLight.directDiffuse.addAssign(lightColor.mul(shade).mul(diffuseColor.rgb).div(Math.PI))
    const halfway = lightDirection.add(positionViewDirection).normalize()
    const highlight = normalView.dot(halfway).clamp().pow(shininess).mul(normalView.dot(lightDirection).clamp())
    reflectedLight.directSpecular.addAssign(lightColor.mul(highlight).mul(specularColor))
    const edge = float(1).sub(normalView.dot(positionViewDirection).clamp()).pow(RIM_POWER)
    const behind = mix(float(RIM_SIDE), float(1), lightDirection.dot(positionViewDirection).negate().clamp())
    reflectedLight.directSpecular.addAssign(lightColor.mul(edge).mul(behind).mul(this.rim).mul(rimStrength).mul(mix(vec3(1), diffuseColor.rgb, 0.5)))
  }
}

/** A clay material: the uniforms in `applyShading` drive every one of them; `rimNode` is how much rim light it takes. */
export class ClayMaterial extends THREE.MeshPhongNodeMaterial {
  rimNode: THREE.Node<'float'> | null = null
  emissiveNode: THREE.Node<'vec3'> | null = null

  setupLightingModel() {
    return new ClayLightingModel(this.rimNode ?? float(0))
  }
}

/**
 * The normal from the map, whose strength follows the relief setting, and `flatten` (0 to 1) lays flat. The map's
 * blue channel is the height, so the normal's z comes from its x and y.
 */
export function reliefNormal(map: THREE.Texture, at: THREE.Node<'vec2'> = uv(), flatten: THREE.Node<'float'> = float(0)) {
  const xy = texture(map, at).grad(dFdx(uv()), dFdy(uv())).rg.mul(2).sub(1).mul(float(1).sub(flatten))
  const z = float(1).sub(xy.dot(xy)).max(0).sqrt()
  return normalMap(vec3(xy, z).mul(0.5).add(0.5), relief)
}

/** The direction to the camera in the tile's texture space (along u, along v, out of the surface), from the screen derivatives of the position and the texture coordinates, as three's tangent frame without tangents. */
function tangentSpaceView() {
  const q0 = dFdx(positionView)
  const q1 = dFdy(positionView)
  const st0 = dFdx(uv())
  const st1 = dFdy(uv())
  const surface = select(frontFacing, normalViewGeometry, normalViewGeometry.negate())
  const q1perp = q1.cross(surface)
  const q0perp = surface.cross(q0)
  const tangent = q1perp.mul(st0.x).add(q0perp.mul(st1.x)).normalize()
  const bitangent = q1perp.mul(st0.y).add(q0perp.mul(st1.y)).normalize()
  return vec3(positionViewDirection.dot(tangent), positionViewDirection.dot(bitangent), positionViewDirection.dot(surface)).normalize()
}

/**
 * Where the view ray meets the tile's height (parallax occlusion): steps from the top of the tile, half the depth
 * above the quad so the tile does not shift on average, into it, along the
 * view, until the ray is under the height in the normal map's blue channel, and stays inside the quad's own rect.
 * The loop has no early exit: WGSL needs the same path for every pixel after it, for the derivatives and shadows.
 */
export function parallaxUV(map: THREE.Texture) {
  return Fn(() => {
    const rect = attribute<'vec4'>(UV_RECT, 'vec4')
    const view = tangentSpaceView()
    const size = rect.zw.sub(rect.xy)
    const count = depthSteps.max(1)
    const step = view.xy.div(view.z.max(0.25)).mul(depth).mul(size).div(count).negate().toVar()
    const at = uv().sub(step.mul(count.mul(0.5))).toVar()
    const layer = float(0).toVar()
    const sunk = float(0).toVar()
    const before = float(0).toVar()
    const searching = float(1).toVar()
    If(depth.greaterThan(0).and(depthSteps.greaterThan(0)), () => {
      Loop({ start: int(0), end: int(count), type: 'int', condition: '<' }, () => {
        If(searching.greaterThan(0), () => {
          sunk.assign(float(1).sub(texture(map, clamp(at, rect.xy, rect.zw)).level(float(0)).b))
          If(sunk.lessThanEqual(layer), () => {
            searching.assign(0)
          }).Else(() => {
            before.assign(sunk.sub(layer))
            at.addAssign(step)
            layer.addAssign(float(1).div(count))
          })
        })
      })
    })
    const after = sunk.sub(layer)
    const between = after.div(after.sub(before).min(-1e-4)).clamp()
    return clamp(mix(at, at.sub(step), select(layer.greaterThan(0), between, float(0))), rect.xy, rect.zw)
  })()
}

/**
 * Where the ground under water is seen: as if lower by the water's depth, along the view, and bent by ripples. The
 * tile wraps inside its rect, as the floor under water repeats.
 */
function underWater(at: THREE.Node<'vec2'>, submerged: THREE.Node<'float'>) {
  const rect = attribute<'vec4'>(UV_RECT, 'vec4')
  const size = rect.zw.sub(rect.xy)
  const view = tangentSpaceView()
  const sink = view.xy.div(view.z.max(0.25)).mul(submerged).mul(liquidDepth).mul(SUNKEN.depth).negate()
  const wave = vec2(mx_noise_float(vec3(positionWorld.xz.mul(SUNKEN.rippleScale), time.mul(SUNKEN.rippleSpeed))), mx_noise_float(vec3(positionWorld.xz.mul(SUNKEN.rippleScale).add(17), time.mul(SUNKEN.rippleSpeed))))
  const shifted = at.sub(rect.xy).div(size).add(sink).add(wave.mul(SUNKEN.ripple).mul(submerged).mul(liquidDepth))
  return select(submerged.mul(liquidDepth).greaterThan(0.001), rect.xy.add(shifted.fract().mul(size)), at)
}

/** The colour of the map scaled by the sprite light, darker on a remembered cell. */
export function spriteColour(map: THREE.Texture, memory: MemoryMask) {
  const colour = texture(map)
  return vec4(colour.rgb.mul(spriteLight).mul(memory.shade), colour.a)
}

/** Cells across and down the memory mask: crawl's levels are 80 by 70. */
const MEMORY_CELLS = 128
const IN_SIGHT = 255
const REMEMBERED = Math.round(MEMORY_SHADE * IN_SIGHT)
/** How far behind a surface its cell is looked for: a wall's face lies on the edge of its cell. */
const INSIDE = 0.02
/** The liquids that wet the ground around them. */
const WETTING: ReadonlySet<LiquidKind> = new Set(['water', 'murky', 'bog', 'mud', 'slime', 'acid'])
/** Cells from a wetting liquid to where the ground is dry again. */
const WET_REACH = 2.5
/** The waters the ground is seen through, and how deep shallow water is on the submersion's 0 to 255. */
const SEE_THROUGH_WATER: ReadonlySet<LiquidKind> = new Set(['water', 'murky'])
const SHALLOW_SUBMERSION = 100
/** How far below the surface the ground under deep water seems, in tiles, and how much the ripples bend it. */
const SUNKEN = { depth: 0.35, ripple: 0.012, rippleScale: 9, rippleSpeed: 0.8 }

/**
 * The cells out of sight, one texel per cell: the level's materials draw a remembered cell darker by it, so a step that
 * changes what is in sight uploads a small texture and leaves the level's geometry as it is.
 */
export class MemoryMask {
  private readonly cells = new Uint8Array(MEMORY_CELLS * MEMORY_CELLS).fill(IN_SIGHT)
  private readonly map = new THREE.DataTexture(this.cells, MEMORY_CELLS, MEMORY_CELLS, THREE.RedFormat)
  /** How light the surface's cell is: 1 in sight, `MEMORY_SHADE` remembered. */
  readonly shade: THREE.Node<'float'>
  private readonly wetCells = new Uint8Array(MEMORY_CELLS * MEMORY_CELLS * 2)
  private readonly wetMap = new THREE.DataTexture(this.wetCells, MEMORY_CELLS, MEMORY_CELLS, THREE.RGFormat)
  private wetKey = ''
  /** How wet the ground is here, 0 to 1: wet by water, bog, mud and slime and drier away from them, smooth between cells. */
  readonly wetness: THREE.Node<'float'>
  /** How deep under clear or murky water the ground is here, 0 to 1 (deep water). */
  readonly submerged: THREE.Node<'float'>
  /** The surface's cell, in cells. */
  readonly cell: THREE.Node<'vec2'>

  constructor() {
    this.map.magFilter = this.map.minFilter = THREE.NearestFilter
    this.map.generateMipmaps = false
    this.map.needsUpdate = true
    const cell = positionWorld.xz.sub(normalWorld.xz.mul(INSIDE)).floor()
    this.cell = cell
    this.shade = texture(this.map, cell.add(0.5).div(MEMORY_CELLS)).r
    this.wetMap.magFilter = this.wetMap.minFilter = THREE.LinearFilter
    this.wetMap.generateMipmaps = false
    this.wetMap.needsUpdate = true
    const wet = texture(this.wetMap, positionWorld.xz.div(MEMORY_CELLS))
    this.wetness = wet.r
    this.submerged = wet.g
  }

  update(scene: Scene) {
    const next = new Uint8Array(this.cells.length).fill(IN_SIGHT)
    for (const cell of scene.cells.values())
      if (cell.visibility === 'remembered' && cell.x >= 0 && cell.y >= 0 && cell.x < MEMORY_CELLS && cell.y < MEMORY_CELLS) next[cell.y * MEMORY_CELLS + cell.x] = REMEMBERED
    this.updateWetness(scene)
    if (next.every((v, i) => v === this.cells[i])) return
    this.cells.set(next)
    this.map.needsUpdate = true
  }

  private updateWetness(scene: Scene) {
    const sources = [...scene.cells.values()].filter((c) => c.liquid && WETTING.has(c.liquid.kind) && c.visibility !== 'unseen' && c.x >= 0 && c.y >= 0 && c.x < MEMORY_CELLS && c.y < MEMORY_CELLS)
    const key = sources.map((c) => `${c.x},${c.y},${c.liquid?.kind},${c.liquid?.deep}`).join(';')
    if (key === this.wetKey) return
    this.wetKey = key
    this.wetCells.fill(0)
    const reach = Math.ceil(WET_REACH)
    for (const source of sources)
      for (let dy = -reach; dy <= reach; dy++)
        for (let dx = -reach; dx <= reach; dx++) {
          const x = source.x + dx
          const y = source.y + dy
          if (x < 0 || y < 0 || x >= MEMORY_CELLS || y >= MEMORY_CELLS) continue
          const wet = Math.max(0, 1 - Math.hypot(dx, dy) / WET_REACH) * 255
          this.wetCells[(y * MEMORY_CELLS + x) * 2] = Math.max(this.wetCells[(y * MEMORY_CELLS + x) * 2], wet)
        }
    for (const source of sources) if (source.liquid && SEE_THROUGH_WATER.has(source.liquid.kind)) this.wetCells[(source.y * MEMORY_CELLS + source.x) * 2 + 1] = source.liquid.deep ? 255 : SHALLOW_SUBMERSION
    this.wetMap.needsUpdate = true
  }

  dispose() {
    this.map.dispose()
    this.wetMap.dispose()
  }
}

/** A colour, darker on a remembered cell. */
export function remembered(colour: THREE.Node<'vec4'>, memory: MemoryMask): THREE.Node<'vec4'> {
  return vec4(colour.rgb.mul(memory.shade), colour.a)
}

/** The textures of an atlas page: the colour, the normal with the height in blue, and the surface (`enhance.ts` `surfaceMap`). */
export interface PageMaps {
  colour: THREE.Texture
  normal: THREE.Texture
  surface: THREE.Texture
}

/** Times the screen derivatives of the texture coordinates for the blurred height the cavity compares with: three mip levels down. */
const CAVITY_SPREAD = 8
const CAVITY_SHARPNESS = 6
/** How much a cavity darkens and an edge lightens at full strength. */
const CAVITY_DARK = 0.45
const EDGE_LIGHT = 0.35
/** How high the puddles of the wettest ground stand, on the height's 0 to 1. */
const PUDDLE_LEVEL = 0.55
const PUDDLE_EDGE = 0.04
/** How dark wet ground gets, and how glossy. */
const WET_DARK = 0.55
const WET_GLOSS = 0.4
/** How fast the light of a glowing part breathes, and how much. */
const GLOW_PULSE = { speed: 1.7, amount: 0.15 }
/** Source texels per glint of sparkle, and how fast the glints come and go. */
const SPARKLE = { size: 1.5, speed: 1.3, threshold: 0.62 }
/** How much rim light the ground takes, against the sprites' 1. */
const GROUND_RIM = 0.15
/** Texels of a page per source texel, for patterns in the art's own texels. */
const PAGE_SOURCE_TEXELS = 1024

/** The light a texel gives: its colour where it glows, breathing, and glints where it sparkles. */
function emission(colour: THREE.Node<'vec3'>, surface: THREE.Node<'vec4'>, at: THREE.Node<'vec2'>, phase: THREE.Node<'float'>) {
  const pulse = float(1).add(time.mul(GLOW_PULSE.speed).add(phase).sin().mul(GLOW_PULSE.amount))
  const glow = colour.mul(surface.g).mul(glowStrength).mul(pulse)
  const glint = smoothstep(SPARKLE.threshold, 1, mx_noise_float(vec3(at.mul(PAGE_SOURCE_TEXELS / SPARKLE.size), time.mul(SPARKLE.speed))).mul(0.5).add(0.5))
  return glow.add(vec3(glint.mul(surface.b).mul(sparkleStrength)))
}

/** A highlight as strong and narrow as the gloss asks. */
function setGloss(material: ClayMaterial, gloss: THREE.Node<'float'>) {
  material.specularNode = vec3(gloss.mul(glossStrength))
  material.shininessNode = mix(float(SHININESS.matte), float(SHININESS.glossy), gloss)
}

/**
 * The ground and the walls: the tile seen through its height (parallax), with dirt in its hollows and wear on its
 * edges, each cell a little different, wet and darker near water with puddles in the low parts, glossy where its
 * surface says, and giving light where it glows; darker on a remembered cell.
 */
export function dressGround(material: ClayMaterial, maps: PageMaps, memory: MemoryMask) {
  const sunk = underWater(parallaxUV(maps.normal), memory.submerged)
  const across = dFdx(uv())
  const down = dFdy(uv())
  const colour = texture(maps.colour, sunk).grad(across, down)
  const surface = texture(maps.surface, sunk).grad(across, down)
  const height = texture(maps.normal, sunk).grad(across, down).b
  const around = texture(maps.normal, sunk).grad(across.mul(CAVITY_SPREAD), down.mul(CAVITY_SPREAD)).b
  const hollow = around.sub(height).mul(CAVITY_SHARPNESS).clamp().mul(cavityStrength)
  const ridge = height.sub(around).mul(CAVITY_SHARPNESS).clamp().mul(cavityStrength)
  const cellHash = hash(memory.cell.x.mul(127.1).add(memory.cell.y.mul(311.7)))
  const varied = float(1).add(cellHash.sub(0.5).mul(2).mul(variationStrength))
  const wet = memory.wetness.mul(wetStrength).clamp()
  const upward = smoothstep(0.5, 0.9, normalWorldGeometry.y)
  const puddle = smoothstep(0, PUDDLE_EDGE, wet.mul(PUDDLE_LEVEL).sub(height).add(0.5).sub(PUDDLE_LEVEL / 2)).mul(upward)
  const shaded = colour.rgb.mul(float(1).sub(hollow.mul(CAVITY_DARK))).mul(float(1).add(ridge.mul(EDGE_LIGHT))).mul(varied).mul(mix(float(1), float(WET_DARK), max(wet.mul(0.6), puddle)))
  material.colorNode = remembered(vec4(shaded, colour.a), memory)
  material.normalNode = reliefNormal(maps.normal, sunk, puddle)
  setGloss(material, max(surface.r, wet.mul(WET_GLOSS), puddle))
  const light = emission(colour.rgb, surface, sunk, cellHash.mul(6.283)).mul(memory.shade)
  material.emissiveNode = light
  material.mrtNode = emissionOutput(light)
  material.rimNode = float(GROUND_RIM)
}

/** The sprites that stand in the level (items, features): lit with the sprite light, glossy and glowing where their surface says, with a rim of light. */
export function dressSprite(material: ClayMaterial, maps: PageMaps, memory: MemoryMask) {
  const surface = texture(maps.surface)
  material.colorNode = spriteColour(maps.colour, memory)
  material.normalNode = reliefNormal(maps.normal)
  setGloss(material, surface.r)
  const light = emission(texture(maps.colour).rgb, surface, uv(), float(0)).mul(memory.shade)
  material.emissiveNode = light
  material.mrtNode = emissionOutput(light)
  material.rimNode = float(1)
}

/** What happens to a monster that crawl shows with a status: each 0 or 1. */
export interface ActorEffects {
  petrified: number
  burning: number
  poisoned: number
  corroded: number
  berserk: number
  glowing: number
  summoned: number
  wet: number
}

export const NO_EFFECTS: ActorEffects = { petrified: 0, burning: 0, poisoned: 0, corroded: 0, berserk: 0, glowing: 0, summoned: 0, wet: 0 }

/** How an actor moves on its own, as its label says. */
export const ACTOR_MOTIONS = { none: 0, breathe: 1, hover: 2, flicker: 3 } as const

/**
 * How an actor's sprite is drawn now: tinted for the flash of a hit, how much of it is left as it fades away (1 is
 * all of it), how it moves on its own (`ACTOR_MOTIONS`) from where in its cycle, and its statuses.
 */
export interface ActorLook {
  tint: THREE.Color
  opacity: number
  motion: number
  phase: number
  effects: ActorEffects
}

const UNTINTED = new THREE.Color(1, 1, 1)
const NOTHING = new THREE.Vector4(0, 0, 0, 0)
const actorLooks = new WeakMap<THREE.Object3D, ActorLook>()
const lookOf = (object: THREE.Object3D | null | undefined) => (object ? actorLooks.get(object) : undefined)
const actorTint = uniform(new THREE.Color(1, 1, 1)).onObjectUpdate(({ object }) => lookOf(object)?.tint ?? UNTINTED)
const actorOpacity = uniform(1).onObjectUpdate(({ object }) => lookOf(object)?.opacity ?? 1)
const actorMotion = uniform(new THREE.Vector2(0, 0)).onObjectUpdate(({ object }, self) => self.value.set(lookOf(object)?.motion ?? 0, lookOf(object)?.phase ?? 0))
let statusesShown = true
const actorHarm = uniform(new THREE.Vector4()).onObjectUpdate(({ object }, self) => {
  const e = statusesShown ? lookOf(object)?.effects : undefined
  return e ? self.value.set(e.petrified, e.burning, e.poisoned, e.corroded) : NOTHING
})
const actorAura = uniform(new THREE.Vector4()).onObjectUpdate(({ object }, self) => {
  const e = statusesShown ? lookOf(object)?.effects : undefined
  return e ? self.value.set(e.berserk, e.glowing, e.summoned, e.wet) : NOTHING
})

/** Draw the mesh, made with an `actorMaterial`, with this look; the look is read each time the mesh is drawn. */
export function setActorLook(mesh: THREE.Object3D, look: ActorLook) {
  actorLooks.set(mesh, look)
}

const HOVER = { lift: 0.05, bob: 0.04, speed: 2.2 }
const FLICKER = { amount: 0.035, speed: 11 }
/** Flames: their noise's size and speed up, how high on the sprite they reach, their colour and their light. */
const FLAME = { scale: vec2(7, 4), rise: 1.6, reach: 0.5, colour: vec3(1, 0.42, 0.08), strength: 0.9 }
const STONE = vec3(0.62, 0.6, 0.57)
const POISON = vec3(0.55, 1.15, 0.45)
const RUST = vec3(0.55, 0.5, 0.3)
const RAGE = vec3(1.35, 0.7, 0.65)
const RAGE_GLOW = vec3(0.7, 0.06, 0.02)
const SUMMONED_GLOW = vec3(0.25, 0.4, 0.7)
/** How much of a see-through sprite, or a summoned one, is drawn. */
const SEE_THROUGH = 0.55
const SUMMONED_SHOWN = 0.8
/** The edge of a dissolving sprite glows like embers, this wide on the dissolve's 0 to 1. */
const DISSOLVE_EDGE = 0.12
const EMBERS = vec3(1.4, 0.5, 0.12)

/**
 * One sprite material for every actor on a page: the flash, the fade, the motion and the statuses are per mesh
 * (`setActorLook`), so a hit, a death or a status never changes the material, and the shader is built once. The
 * fade is a dissolve, which stays opaque; a see-through monster (a ghost) is drawn in part the same way. A petrified
 * monster turns to grey stone, a burning one has flames, a poisoned one pulses green, a corroded one has rust, a
 * berserk one glows red, a glowing one gives light, a summoned one shimmers, a waterlogged one is wet and glossy.
 */
export function actorMaterial(sprite: ClayMaterial, maps: PageMaps): ClayMaterial {
  const material = sprite.clone()
  material.alphaTest = sprite.alphaTest
  const colour = texture(maps.colour)
  const surface = texture(maps.surface)
  const [motion, phase] = [actorMotion.x, actorMotion.y]
  const hover = motion.equal(ACTOR_MOTIONS.hover).select(time.mul(HOVER.speed).add(phase).sin().mul(HOVER.bob).add(HOVER.lift), float(0))
  const flicker = motion.equal(ACTOR_MOTIONS.flicker).select(time.mul(FLICKER.speed).add(phase).sin().mul(FLICKER.amount), float(0))
  material.positionNode = vec3(positionLocal.x.mul(float(1).add(flicker)), positionLocal.y.add(hover), positionLocal.z)
  const petrified = actorHarm.x
  const burning = actorHarm.y
  const poisoned = actorHarm.z
  const corroded = actorHarm.w
  const berserk = actorAura.x
  const glowing = actorAura.y
  const summoned = actorAura.z
  const soaked = actorAura.w
  const texel = uv().mul(PAGE_SOURCE_TEXELS)
  const grey = colour.rgb.dot(vec3(0.299, 0.587, 0.114))
  const stone = STONE.mul(grey.mul(0.8).add(0.25))
  const pulse = time.mul(3).add(phase).sin().mul(0.5).add(0.5)
  const rust = smoothstep(0.55, 0.75, mx_noise_float(texel.mul(0.35)).mul(0.5).add(0.5)).mul(corroded)
  let tinted: THREE.Node<'vec3'> = mix(colour.rgb, stone, petrified)
  tinted = mix(tinted, tinted.mul(POISON), poisoned.mul(pulse.mul(0.4).add(0.3)))
  tinted = mix(tinted, tinted.mul(RUST), rust)
  tinted = mix(tinted, tinted.mul(RAGE), berserk)
  tinted = mix(tinted, tinted.mul(WET_DARK + 0.2), soaked)
  material.colorNode = vec4(tinted.mul(spriteLight).mul(actorTint), colour.a)
  setGloss(material, mix(max(surface.r, soaked.mul(WET_GLOSS)), float(0), petrified))
  const flameNoise = mx_noise_float(vec3(texel.div(32).mul(FLAME.scale).sub(vec2(0, time.mul(FLAME.rise))), phase)).mul(0.5).add(0.5)
  const flames = smoothstep(0.6, 0.8, flameNoise.add(0.25).sub(positionLocal.y.mul(FLAME.reach)))
  const edge = float(1).sub(normalView.dot(positionViewDirection).clamp()).pow(2)
  const grain = interleavedGradientNoise(screenCoordinate.xy)
  const embers = smoothstep(actorOpacity.sub(DISSOLVE_EDGE), actorOpacity, grain).mul(actorOpacity.lessThan(1).select(float(1), float(0)))
  const light = emission(colour.rgb, surface, uv(), phase)
    .mul(float(1).sub(petrified))
    .add(EMBERS.mul(embers))
    .add(FLAME.colour.mul(flames).mul(burning).mul(FLAME.strength))
    .add(RAGE_GLOW.mul(edge).mul(berserk).mul(pulse.mul(0.5).add(0.5)))
    .add(colour.rgb.mul(0.5).mul(glowing))
    .add(SUMMONED_GLOW.mul(edge).mul(summoned))
  material.emissiveNode = light
  material.mrtNode = emissionOutput(light)
  material.rimNode = float(1)
  const shown = actorOpacity.mul(mix(float(1), float(SEE_THROUGH), surface.a)).mul(mix(float(1), float(SUMMONED_SHOWN).add(pulse.mul(1 - SUMMONED_SHOWN)), summoned))
  material.maskNode = grain.lessThan(shown)
  return material
}

/** The layers the shadow cameras see: only the meshes that cast a shadow are on them, so a shadow map is drawn without going through the rest of the world. The actors have a layer of their own, as the torches' shadows can leave them out. */
export const SHADOW_LAYER = 1
export const ACTOR_SHADOW_LAYER = 2

/** Let the mesh cast a shadow or not, on a shadow cameras' layer to match. */
export function castShadow(mesh: THREE.Object3D, casts: boolean, layer = SHADOW_LAYER) {
  mesh.castShadow = casts
  if (casts) mesh.layers.enable(layer)
  else mesh.layers.disable(layer)
}
