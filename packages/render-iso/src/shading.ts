import * as THREE from 'three/webgpu'
import { diffuseColor, normalMap, normalView, select, texture, uniform, vec4 } from 'three/tsl'

export interface ShadingSettings {
  /** How far the light wraps around a surface: 0 is plain diffuse light, 1 lights even surfaces that face away (a soft, clay look). */
  wrap: number
  /** Flat tones of light; 0 for a smooth change. */
  bands: number
  /** Strength of the bumps from the tile normal maps. */
  relief: number
  /** Light on sprites (monsters, items): below 1 keeps the detail of bright sprites in strong light. */
  sprites: number
}

const wrap = uniform(0)
const bands = uniform(0)
const relief = uniform(1)
const spriteLight = uniform(1)

export function applyShading(settings: ShadingSettings) {
  wrap.value = settings.wrap
  bands.value = settings.bands
  relief.value = settings.relief
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

/** A clay material with a normal map whose strength follows the relief setting. */
export function reliefNormal(map: THREE.Texture) {
  return normalMap(texture(map), relief)
}

/** The colour of the map scaled by the sprite light. */
export function spriteColour(map: THREE.Texture) {
  const colour = texture(map)
  return vec4(colour.rgb.mul(spriteLight), colour.a)
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
