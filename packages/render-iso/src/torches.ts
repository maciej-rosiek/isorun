import * as THREE from 'three/webgpu'
import { float, instancedBufferAttribute } from 'three/tsl'
import { radialTexture } from './materials.js'
import type { ModelPart } from './assets.js'

/** Where a wall torch's model stands, and where its flame glows. */
export interface TorchPlacement {
  model: THREE.Matrix4
  glow: THREE.Vector3
}

const GLOW_SIZE = 0.45
/** Torches room is made for at first, so a level rarely makes the instanced buffers again. */
const FIRST_ROOM = 64

/** Every wall torch of the level in two draws: the models as one instanced mesh, and their glows as one instanced sprite. */
export class TorchLayer {
  readonly group = new THREE.Group()
  private models: THREE.InstancedMesh | null = null
  private part: ModelPart | null = null
  private readonly glowMaterial = new THREE.SpriteNodeMaterial({ map: radialTexture('rgba(255,190,110,1)'), color: 0xffb060, blending: THREE.AdditiveBlending, depthWrite: false, transparent: true })
  private glowPositions = new THREE.InstancedBufferAttribute(new Float32Array(FIRST_ROOM * 3), 3)
  private readonly glows = new THREE.Sprite(this.glowMaterial)
  private readonly matrix = new THREE.Matrix4()

  constructor() {
    this.glowMaterial.positionNode = instancedBufferAttribute(this.glowPositions)
    this.glowMaterial.scaleNode = float(GLOW_SIZE)
    this.glows.count = 0
    this.glows.frustumCulled = false
    this.glows.visible = false
    this.group.add(this.glows)
  }

  /** Draw these torches, with this model. */
  set(part: ModelPart, torches: readonly TorchPlacement[]) {
    if (part !== this.part || !this.models || this.models.instanceMatrix.count < torches.length) {
      if (this.models) this.group.remove(this.models)
      this.models?.dispose()
      this.part = part
      this.models = new THREE.InstancedMesh(part.geometry, part.material, Math.max(FIRST_ROOM, torches.length * 2))
      this.models.receiveShadow = true
      this.models.frustumCulled = false
      this.group.add(this.models)
    }
    torches.forEach((torch, i) => this.models?.setMatrixAt(i, this.matrix.multiplyMatrices(torch.model, part.matrix)))
    this.models.count = torches.length
    this.models.instanceMatrix.needsUpdate = true
    if (this.glowPositions.count < torches.length) {
      this.glowPositions = new THREE.InstancedBufferAttribute(new Float32Array(torches.length * 2 * 3), 3)
      this.glowMaterial.positionNode = instancedBufferAttribute(this.glowPositions)
      this.glowMaterial.needsUpdate = true
    }
    torches.forEach((torch, i) => this.glowPositions.setXYZ(i, torch.glow.x, torch.glow.y, torch.glow.z))
    this.glowPositions.needsUpdate = true
    this.glows.count = torches.length
    this.glows.visible = torches.length > 0
  }

  dispose() {
    this.models?.dispose()
    this.glowMaterial.map?.dispose()
    this.glowMaterial.dispose()
  }
}
