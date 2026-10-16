import * as THREE from 'three/webgpu'
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js'

/** Kit pieces are authored on a 4-unit grid; a cell is 1. */
export const KIT_SCALE = 0.25

/** The 3D models: a torch for the walls (KayKit Dungeon, CC0), loaded once and cloned per use. */
export class IsoAssets {
  private constructor(private readonly torchTemplate: THREE.Object3D) {}

  static async load(baseUrl: string): Promise<IsoAssets> {
    const gltf = await new GLTFLoader().loadAsync(`${baseUrl}/torch_mounted.glb`)
    gltf.scene.traverse((o) => {
      o.receiveShadow = true
    })
    return new IsoAssets(gltf.scene)
  }

  torch(): THREE.Object3D {
    return this.torchTemplate.clone()
  }
}
