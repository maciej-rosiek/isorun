fn voxel_sway(v: VoxelIn) -> vec3<f32> {
    return vec3<f32>(0.0);
}

fn voxel_material_out(in: VoxelOut, at: vec2<f32>, s: Surface, light: vec3<f32>, kind: u32) -> FragmentOut {
    return voxel_lit(s, light);
}
