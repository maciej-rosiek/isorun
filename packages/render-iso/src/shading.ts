import * as THREE from 'three/webgpu'
import { Fn, If, Loop, attribute, clamp, diffuseColor, dFdx, dFdy, float, frontFacing, interleavedGradientNoise, mix, normalMap, normalView, normalViewGeometry, positionView, positionViewDirection, screenCoordinate, select, texture, uniform, uv, vec3, vec4 } from 'three/tsl'
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
}

const wrap = uniform(0)
const bands = uniform(0)
const relief = uniform(1)
const depth = uniform(0)
const DEPTH_STEPS = 16
const spriteLight = uniform(1)

export function applyShading(settings: ShadingSettings) {
  wrap.value = settings.wrap
  bands.value = settings.bands
  relief.value = settings.relief
  depth.value = settings.depth
  spriteLight.value = settings.sprites
}

type DirectLight = THREE.LightingModelDirectInput & {
  lightDirection: THREE.Node<'vec3'>
  lightColor: THREE.Node<'vec3'>
  reflectedLight: { directDiffuse: THREE.Node<'vec3'> }
}

/** Matte diffuse light that wraps around surfaces and can step into flat tones; no specular. */
class ClayLightingModel extends THREE.PhongLightingModel {
  constructor() {
    super(false)
  }

  direct({ lightDirection, lightColor, reflectedLight }: DirectLight) {
    const wrapped = normalView.dot(lightDirection).add(wrap).div(wrap.add(1)).clamp()
    const banded = wrapped.mul(bands).ceil().div(bands.max(1))
    const shade = select(bands.greaterThan(0.5), banded, wrapped)
    reflectedLight.directDiffuse.addAssign(lightColor.mul(shade).mul(diffuseColor.rgb).div(Math.PI))
  }
}

/** A matte material lit by the clay light; the uniforms in `applyShading` drive every one of them. */
export class ClayMaterial extends THREE.MeshPhongNodeMaterial {
  setupLightingModel() {
    return new ClayLightingModel()
  }
}

/**
 * The normal from the map, whose strength follows the relief setting. The map's blue channel is the height, so
 * the normal's z comes from its x and y.
 */
export function reliefNormal(map: THREE.Texture, at: THREE.Node<'vec2'> = uv()) {
  const xy = texture(map, at).grad(dFdx(uv()), dFdy(uv())).rg.mul(2).sub(1)
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
    const step = view.xy.div(view.z.max(0.25)).mul(depth).mul(size).div(DEPTH_STEPS).negate().toVar()
    const at = uv().sub(step.mul(DEPTH_STEPS / 2)).toVar()
    const layer = float(0).toVar()
    const sunk = float(0).toVar()
    const before = float(0).toVar()
    const searching = float(1).toVar()
    If(depth.greaterThan(0), () => {
      Loop(DEPTH_STEPS, () => {
        If(searching.greaterThan(0), () => {
          sunk.assign(float(1).sub(texture(map, clamp(at, rect.xy, rect.zw)).level(float(0)).b))
          If(sunk.lessThanEqual(layer), () => {
            searching.assign(0)
          }).Else(() => {
            before.assign(sunk.sub(layer))
            at.addAssign(step)
            layer.addAssign(1 / DEPTH_STEPS)
          })
        })
      })
    })
    const after = sunk.sub(layer)
    const between = after.div(after.sub(before).min(-1e-4)).clamp()
    return clamp(mix(at, at.sub(step), select(layer.greaterThan(0), between, float(0))), rect.xy, rect.zw)
  })()
}

/** The colour of the map scaled by the sprite light. */
export function spriteColour(map: THREE.Texture) {
  const colour = texture(map)
  return vec4(colour.rgb.mul(spriteLight), colour.a)
}

/** How an actor's sprite is drawn now: tinted for the flash of a hit, and how much of it is left as it fades away (1 is all of it). */
export interface ActorLook {
  tint: THREE.Color
  opacity: number
}

const UNTINTED = new THREE.Color(1, 1, 1)
const actorLooks = new WeakMap<THREE.Object3D, ActorLook>()
const actorTint = uniform(new THREE.Color(1, 1, 1)).onObjectUpdate(({ object }) => (object && actorLooks.get(object)?.tint) ?? UNTINTED)
const actorOpacity = uniform(1).onObjectUpdate(({ object }) => (object && actorLooks.get(object)?.opacity) ?? 1)

/** Draw the mesh, made with an `actorMaterial`, with this look; the look is read each time the mesh is drawn. */
export function setActorLook(mesh: THREE.Object3D, look: ActorLook) {
  actorLooks.set(mesh, look)
}

/**
 * One sprite material for every actor on a page: the flash and the fade are per mesh (`setActorLook`), so a hit or
 * a death never changes the material, and the shader is built once. The fade is a dissolve, which stays opaque.
 */
export function actorMaterial(sprite: ClayMaterial, map: THREE.Texture): ClayMaterial {
  const material = sprite.clone()
  material.alphaTest = sprite.alphaTest
  const colour = texture(map)
  material.colorNode = vec4(colour.rgb.mul(spriteLight).mul(actorTint), colour.a)
  material.maskNode = interleavedGradientNoise(screenCoordinate.xy).lessThan(actorOpacity)
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
