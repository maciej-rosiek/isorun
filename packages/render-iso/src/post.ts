import * as THREE from 'three/webgpu'
import { abs, convertToTexture, float, length, luminance, max, min, mix, mrt, normalViewGeometry, output, packNormalToRGB, pass, sample, saturation, screenSize, screenUV, select, smoothstep, uniform, unpackRGBToNormal, vec2, vec3, vec4 } from 'three/tsl'
import { ao, type default as GTAONode } from 'three/addons/tsl/display/GTAONode.js'
import { bloom } from 'three/addons/tsl/display/BloomNode.js'
import { gaussianBlur } from 'three/addons/tsl/display/GaussianBlurNode.js'

export interface ScreenLightSettings {
  /** Ambient occlusion: soft contact shadows in corners and at the foot of walls. */
  ao: { enabled: boolean; intensity: number; radius: number }
  /** Dark lines where the depth or the surface direction changes: around walls, steps and sprites. */
  outline: { enabled: boolean; thickness: number; strength: number; colour: number }
  /** A Kuwahara filter on the whole screen, so the image looks painted; `radius` is pixels per step of the brush. */
  painted: { enabled: boolean; radius: number }
  /** Big square pixels over the whole screen; `size` is screen pixels per square at the default zoom, and the squares follow the zoom and the map. */
  pixelate: { enabled: boolean; size: number }
  /** A soft glow around bright light: flames, glowing items. */
  bloom: { enabled: boolean; strength: number; radius: number; threshold: number }
  /** The top and the bottom of the screen blur, so the dungeon looks like a small model. */
  tiltShift: { enabled: boolean; blur: number; focus: number; falloff: number }
  /** Colour grade: saturation, a tint for the dark and for the bright parts, and darker screen corners. */
  grade: { saturation: number; shadows: number; highlights: number; vignette: number }
}

/** The tilt-shift blur's kernel size (three's GaussianBlurNode: 3 + 2 * sigma taps each side): 9 taps a pass. */
const BLUR_SIGMA = 1
/** The Blur setting was scaled for a kernel of sigma 4, 21 taps a pass with most of them a fraction of a texel apart; the taps step this much farther, so a setting blurs as far as before with fewer of them. */
const BLUR_STEP = (3 + 2 * 4) / (3 + 2 * BLUR_SIGMA)
const BLUR_RESOLUTION = 0.5
const PAINT_STEPS = 3

/** For `material.mrtNode`: the screen effects leave this material as it is drawn. */
export function overlayOutput() {
  return mrt({ overlay: float(1) })
}

export function emissionOutput(colour: THREE.Node<'vec3'>) {
  return mrt({ emission: vec4(colour, 1) })
}
const EMISSION_RADIUS = 0.6
const OUTLINE_DEPTH = 0.25
const OUTLINE_NORMAL = 0.35
const DANGER_EDGE_START = 0.6
const DANGER_EDGE_END = 1.3
const DANGER_STRENGTH = 0.3
const DANGER_COLOUR = vec3(0.45, 0.03, 0.03)

/**
 * The scene pass with the screen effects on top: GTAO, a painted filter, outlines, bloom, tilt-shift blur and a colour grade; then the overlay materials (`overlayOutput`) as they are.
 * With no effect but GTAO before it, the tilt-shift blurs the scene's own colour and shades the blur with the GTAO after: blurring the shaded image would draw it whole once more
 * into a texture of its own, which on a big screen costs as much as the rest of the effects together.
 */
export class IsoPost {
  private readonly pipeline: THREE.RenderPipeline
  private readonly aoIntensity = uniform(1)
  private readonly outlineThickness = uniform(1)
  private readonly outlineStrength = uniform(1)
  private readonly outlineColour = uniform(new THREE.Color(0, 0, 0))
  private readonly depthRange = uniform(1)
  private readonly paintRadius = uniform(1)
  private readonly pixelSize = uniform(1)
  private readonly pixelScale = uniform(1)
  private readonly pixelOrigin = uniform(new THREE.Vector2())
  private readonly bloomStrength = uniform(1)
  private readonly bloomRadius = uniform(0)
  private readonly bloomThreshold = uniform(0)
  private readonly emissionStrength = uniform(0)
  private emitting = false
  private readonly blur = uniform(1)
  private readonly focus = uniform(0.2)
  private readonly falloff = uniform(0.3)
  private readonly saturation = uniform(1)
  private readonly shadows = uniform(new THREE.Color(1, 1, 1))
  private readonly highlights = uniform(new THREE.Color(1, 1, 1))
  private readonly vignette = uniform(0)
  private readonly danger = uniform(0)
  private aoNode: GTAONode | null = null
  private aoResolution = 1
  private shape = ''

  constructor(
    renderer: THREE.WebGPURenderer,
    private readonly world: THREE.Scene,
    private camera: THREE.OrthographicCamera | THREE.PerspectiveCamera,
  ) {
    this.pipeline = new THREE.RenderPipeline(renderer, pass(world, camera))
  }

  /** Draw through another camera: the screen effects are built again for it. */
  setCamera(camera: THREE.OrthographicCamera | THREE.PerspectiveCamera) {
    if (camera === this.camera) return
    this.camera = camera
    this.shape = ''
  }

  /** The size of the ambient occlusion's image, a part of the screen's size; 0 turns it off. */
  setAoResolution(scale: number) {
    if (scale > 0 !== this.aoResolution > 0) this.shape = ''
    this.aoResolution = scale
    if (this.aoNode) this.aoNode.resolutionScale = scale
  }

  configure(settings: ScreenLightSettings) {
    const shape = [{ enabled: this.aoOn(settings) }, { enabled: this.emitting }, settings.outline, settings.painted, settings.pixelate, settings.bloom, settings.tiltShift].map((s) => s.enabled).join('|')
    if (shape !== this.shape) {
      this.shape = shape
      this.build(settings)
    }
    this.aoIntensity.value = settings.ao.intensity
    if (this.aoNode) this.aoNode.radius.value = settings.ao.radius
    this.outlineThickness.value = settings.outline.thickness
    this.outlineStrength.value = settings.outline.strength
    this.outlineColour.value.set(settings.outline.colour)
    this.paintRadius.value = settings.painted.radius
    this.pixelSize.value = settings.pixelate.size
    this.bloomStrength.value = settings.bloom.strength
    this.bloomRadius.value = settings.bloom.radius
    this.bloomThreshold.value = settings.bloom.threshold
    this.blur.value = settings.tiltShift.blur
    this.focus.value = settings.tiltShift.focus
    this.falloff.value = settings.tiltShift.falloff
    this.saturation.value = settings.grade.saturation
    this.shadows.value.set(settings.grade.shadows)
    this.highlights.value.set(settings.grade.highlights)
    this.vignette.value = settings.grade.vignette
  }

  setEmission(strength: number) {
    this.emissionStrength.value = strength
    if (strength > 0 === this.emitting) return
    this.emitting = strength > 0
    this.shape = ''
  }

  /** How red the screen edge is, 0 to 1. */
  setDanger(amount: number) {
    this.danger.value = amount
  }

  /** `pixelGrid`: how much larger the pixel look's squares are drawn (the zoom), and where on the screen, in device pixels, the map's origin is, so the squares move with the map. */
  render(pixelGrid: { scale: number; x: number; y: number }, settings: ScreenLightSettings) {
    if (this.shape === '') this.configure(settings)
    this.pixelScale.value = pixelGrid.scale
    this.pixelOrigin.value.set(pixelGrid.x, pixelGrid.y)
    this.depthRange.value = this.camera.far - this.camera.near
    this.pipeline.render()
  }

  dispose() {
    this.pipeline.dispose()
  }

  private build(settings: ScreenLightSettings) {
    const scenePass = pass(this.world, this.camera)
    this.aoNode = null
    const aoOn = this.aoOn(settings)
    const needsNormals = aoOn || settings.outline.enabled
    scenePass.setMRT(mrt({ output, overlay: float(0), ...(needsNormals ? { normal: packNormalToRGB(normalViewGeometry) } : {}), ...(this.emitting ? { emission: vec4(0) } : {}) }))
    const colour = scenePass.getTextureNode('output')
    const depthTexture = scenePass.getTextureNode('depth')
    let image: THREE.Node<'vec4'> = colour
    let shade = (lit: THREE.Node<'vec4'>) => lit
    if (aoOn) {
      const normalTexture = scenePass.getTextureNode('normal')
      this.aoNode = ao(depthTexture, sample((uv) => unpackRGBToNormal(normalTexture.sample(uv))), this.camera)
      this.aoNode.resolutionScale = this.aoResolution
      const occlusion = mix(float(1), this.aoNode.getTextureNode().r, this.aoIntensity)
      shade = (lit) => vec4(lit.rgb.mul(occlusion), lit.a)
      image = shade(colour)
    }
    const onlyShaded = !settings.painted.enabled && !settings.outline.enabled && !settings.pixelate.enabled && !settings.bloom.enabled && !this.emitting
    if (settings.painted.enabled) image = this.paint(image)
    if (settings.outline.enabled) {
      const normalTexture = scenePass.getTextureNode('normal')
      const depthAt = (offset: THREE.Node<'vec2'>) => depthTexture.sample(screenUV.add(offset)).r.mul(this.depthRange)
      const normalAt = (offset: THREE.Node<'vec2'>) => unpackRGBToNormal(normalTexture.sample(screenUV.add(offset)).rgb)
      const centreDepth = depthAt(vec2(0, 0))
      const centreNormal = normalAt(vec2(0, 0))
      let edge: THREE.Node<'float'> = float(0)
      for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
        const offset = vec2(dx, dy).mul(this.outlineThickness).div(screenSize)
        const farther = smoothstep(OUTLINE_DEPTH, OUTLINE_DEPTH * 2, depthAt(offset).sub(centreDepth))
        const turned = smoothstep(OUTLINE_NORMAL, OUTLINE_NORMAL * 2, float(1).sub(centreNormal.dot(normalAt(offset))))
        edge = max(edge, max(farther, turned))
      }
      image = vec4(mix(image.rgb, this.outlineColour, edge.mul(this.outlineStrength)), image.a)
    }
    if (settings.pixelate.enabled) image = this.pixelate(image)
    if (settings.bloom.enabled) image = image.add(bloom(colour, this.bloomStrength, this.bloomRadius, this.bloomThreshold))
    if (this.emitting) image = image.add(bloom(scenePass.getTextureNode('emission'), this.emissionStrength, EMISSION_RADIUS, 0))
    if (settings.tiltShift.enabled) {
      const blurred = onlyShaded ? shade(gaussianBlur(colour, this.blur.mul(BLUR_STEP), BLUR_SIGMA, { resolutionScale: BLUR_RESOLUTION })) : gaussianBlur(image, this.blur.mul(BLUR_STEP), BLUR_SIGMA, { resolutionScale: BLUR_RESOLUTION })
      const away = smoothstep(this.focus, this.focus.add(this.falloff), abs(screenUV.y.sub(0.5)).mul(2))
      image = mix(image, blurred, away)
    }
    const graded = saturation(image.rgb, this.saturation)
    const tint = mix(this.shadows.div(luminance(this.shadows)), this.highlights.div(luminance(this.highlights)), smoothstep(0, 0.6, luminance(graded)))
    const corners = float(1).sub(length(screenUV.sub(0.5)).mul(1.414).pow(2).mul(this.vignette))
    const overlay = scenePass.getTextureNode('overlay').r.clamp()
    const dangerEdge = smoothstep(DANGER_EDGE_START, DANGER_EDGE_END, length(screenUV.sub(0.5)).mul(1.414)).mul(this.danger).mul(DANGER_STRENGTH)
    const shown = mix(graded.mul(tint).mul(corners), DANGER_COLOUR, dangerEdge)
    this.pipeline.outputNode = vec4(mix(shown, colour.rgb, overlay), image.a)
    this.pipeline.needsUpdate = true
  }

  private aoOn(settings: ScreenLightSettings) {
    return settings.ao.enabled && this.aoResolution > 0
  }

  /** Every pixel takes the colour at the centre of its square. */
  private pixelate(image: THREE.Node<'vec4'>): THREE.Node<'vec4'> {
    const source = convertToTexture(image)
    const size = this.pixelSize.mul(this.pixelScale).max(1)
    const square = screenUV.mul(screenSize).sub(this.pixelOrigin).div(size).floor().add(0.5)
    return source.sample(square.mul(size).add(this.pixelOrigin).div(screenSize))
  }

  /** Kuwahara: every pixel takes the mean colour of the most even of the four squares that meet at it. */
  private paint(image: THREE.Node<'vec4'>): THREE.Node<'vec4'> {
    const source = convertToTexture(image)
    const step = vec2(this.paintRadius, this.paintRadius).div(screenSize)
    const texels = new Map<string, { colour: THREE.Node<'vec3'>; light: THREE.Node<'float'> }>()
    const texel = (x: number, y: number) => {
      const key = `${x},${y}`
      const cached = texels.get(key)
      if (cached) return cached
      const colour = source.sample(screenUV.add(vec2(x, y).mul(step))).rgb
      const entry = { colour, light: luminance(colour) }
      texels.set(key, entry)
      return entry
    }
    let best: THREE.Node<'vec3'> = vec3(0)
    let bestVariance: THREE.Node<'float'> = float(1e9)
    for (const [sx, sy] of [[-1, -1], [1, -1], [-1, 1], [1, 1]]) {
      let sum: THREE.Node<'vec3'> = vec3(0)
      let light: THREE.Node<'float'> = float(0)
      let lightSquared: THREE.Node<'float'> = float(0)
      for (let i = 0; i <= PAINT_STEPS; i++)
        for (let j = 0; j <= PAINT_STEPS; j++) {
          const t = texel(i * sx, j * sy)
          sum = sum.add(t.colour)
          light = light.add(t.light)
          lightSquared = lightSquared.add(t.light.mul(t.light))
        }
      const n = (PAINT_STEPS + 1) * (PAINT_STEPS + 1)
      const mean = light.div(n)
      const variance = lightSquared.div(n).sub(mean.mul(mean))
      best = select(variance.lessThan(bestVariance), sum.div(n), best)
      bestVariance = min(variance, bestVariance)
    }
    return vec4(best, image.a)
  }
}
