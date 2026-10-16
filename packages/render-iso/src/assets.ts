import * as THREE from 'three/webgpu'
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js'

/** Kit pieces are authored on a 4-unit grid; a cell is 1. */
export const KIT_SCALE = 0.25

/** One mesh of a model: what an instanced mesh draws it from, with the place the model puts it at. */
export interface ModelPart {
  geometry: THREE.BufferGeometry
  material: THREE.Material
  matrix: THREE.Matrix4
}

/** The 3D models: a torch for the walls (KayKit Dungeon, CC0), loaded once and drawn instanced. */
export class IsoAssets {
  private constructor(readonly torch: ModelPart) {}

  static async load(baseUrl: string): Promise<IsoAssets> {
    const gltf = await new GLTFLoader().loadAsync(`${baseUrl}/torch_mounted.glb`)
    gltf.scene.updateMatrixWorld(true)
    let torch: ModelPart | null = null
    gltf.scene.traverse((o) => {
      if (!torch && o instanceof THREE.Mesh && o.material instanceof THREE.Material) torch = { geometry: o.geometry, material: o.material, matrix: o.matrixWorld.clone() }
    })
    if (!torch) throw new Error('torch_mounted.glb has no mesh')
    return new IsoAssets(torch)
  }
}
