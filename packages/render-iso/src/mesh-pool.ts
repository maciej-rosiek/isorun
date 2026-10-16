import * as THREE from 'three/webgpu'

/**
 * Meshes kept for use again. Three frees a mesh's render objects (its pipelines' bindings and uniform buffers) only
 * when the mesh's material is disposed, never when the mesh leaves the scene or its geometry is disposed; the level's
 * materials are shared and kept, so each mesh dropped with one would stay in three's caches for good. A mesh given
 * back here is taken again for the same material, so its render objects serve the next one.
 */
export class MeshPool {
  private readonly free = new Map<THREE.Material, THREE.Mesh[]>()

  /** A mesh drawing `geometry` with `material`, at the origin, casting and receiving no shadow, on layer 0 alone. */
  take(geometry: THREE.BufferGeometry, material: THREE.Material): THREE.Mesh {
    const mesh = this.free.get(material)?.pop()
    if (!mesh) return new THREE.Mesh(geometry, material)
    mesh.geometry = geometry
    return mesh
  }

  /** Take the meshes under `root` out of the scene for use again; their geometries are the caller's to dispose. */
  give(root: THREE.Object3D) {
    root.removeFromParent()
    const meshes: THREE.Mesh[] = []
    root.traverse((object) => {
      if (object instanceof THREE.Mesh && !(object instanceof THREE.InstancedMesh)) meshes.push(object)
    })
    for (const mesh of meshes) {
      const material = mesh.material
      if (!(material instanceof THREE.Material)) continue
      mesh.removeFromParent()
      mesh.position.set(0, 0, 0)
      mesh.rotation.set(0, 0, 0)
      mesh.scale.set(1, 1, 1)
      mesh.castShadow = false
      mesh.receiveShadow = false
      mesh.layers.set(0)
      mesh.renderOrder = 0
      mesh.visible = true
      mesh.matrixWorldAutoUpdate = true
      mesh.updateMatrix()
      const free = this.free.get(material) ?? []
      free.push(mesh)
      this.free.set(material, free)
    }
  }
}
