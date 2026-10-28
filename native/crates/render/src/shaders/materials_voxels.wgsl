@group(2) @binding(0) var<uniform> material: Materials;
@group(2) @binding(1) var material_cells: texture_2d<f32>;
@group(2) @binding(2) var material_heat: texture_2d<f32>;

fn class_at(at: vec2<f32>) -> u32 {
    return class_of(textureLoad(normal_map, page_texel(at, textureDimensions(normal_map)), 0).a);
}

fn voxel_sway(v: VoxelIn) -> vec3<f32> {
    if (material.sway <= 0.0 || v.origin.w < VOXEL_FIGURE || class_at(mix(v.uv_rect.xy, v.uv_rect.zw, vec2<f32>(0.5))) != CLASS_PLANT) {
        return vec3<f32>(0.0);
    }
    let foot = v.origin.xyz + v.axis_x.xyz * 0.5 + v.axis_y.xyz;
    return v.axis_x.xyz * sway_at(foot, saturate1(1.0 - v.position.y));
}

fn voxel_material_out(in: VoxelOut, at: vec2<f32>, s: Surface, light: vec3<f32>, kind: u32) -> FragmentOut {
    var m: MaterialIn;
    m.made_of = class_at(at);
    m.kind = kind;
    m.world = in.world;
    m.ground_normal = normalize(in.face);
    m.shade = memory_shade(memory_cell(in.world, m.ground_normal));
    m.facet = floor(at * vec2<f32>(textureDimensions(normal_map)));
    m.grain = m.facet;
    m.inside = s.diffuse;
    m.pixel = in.clip.xy;
    let lit = material_lit(m, s, light);
    return voxel_outputs(s, lit.colour, lit.light);
}
