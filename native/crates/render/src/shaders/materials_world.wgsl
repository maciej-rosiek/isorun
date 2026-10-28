@group(3) @binding(0) var<uniform> material: Materials;
@group(3) @binding(1) var material_cells: texture_2d<f32>;
@group(3) @binding(2) var material_heat: texture_2d<f32>;

const FACET_TEXELS: f32 = 2.0;

fn class_at(at: vec2<f32>) -> u32 {
    return class_of(textureLoad(normal_map, page_texel(at, textureDimensions(normal_map)), 0).a);
}

fn material_at(in: VertexOut, at: vec2<f32>) -> vec2<f32> {
    if (material.sway <= 0.0 || class_at(at) != CLASS_PLANT) {
        return at;
    }
    let rect = in.uv_rect;
    let bend = sway_at(in.world, saturate1(in.local_y)) * (rect.z - rect.x);
    return vec2<f32>(clamp(at.x + bend, rect.x, rect.z), at.y);
}

fn material_out(in: VertexOut, at: vec2<f32>, s: Surface, light: vec3<f32>, alpha: f32, face: f32, kind: u32) -> FragmentOut {
    var m: MaterialIn;
    m.made_of = class_at(at);
    m.kind = kind;
    m.world = in.world;
    m.ground_normal = normalize(in.normal_world);
    m.shade = memory_shade(memory_cell(in.world, m.ground_normal));
    m.facet = floor(at * (PAGE_SOURCE_TEXELS / FACET_TEXELS));
    m.grain = floor(at * PAGE_SOURCE_TEXELS);
    m.inside = s.diffuse;
    if (m.made_of == CLASS_CRYSTAL) {
        m.inside = seen_inside(in, at, s.view_dir, face) * m.shade * in.colour;
    }
    m.pixel = in.clip.xy;
    let lit = material_lit(m, s, light);
    return outputs(lit.colour, alpha, in, face, lit.light);
}

fn seen_inside(in: VertexOut, at: vec2<f32>, view_dir: vec3<f32>, face: f32) -> vec3<f32> {
    let rect = in.uv_rect;
    let size = rect.zw - rect.xy;
    let inside = tangent_space_view(in.view_pos, normalize(in.normal_view) * face, in.uv, view_dir);
    let deep = rect.xy + fract((at - rect.xy) / size - inside.xy / max(inside.z, 0.3) * material.crystal_depth * 0.5) * size;
    return textureSampleLevel(colour_map, page_sampler, deep, 0.0).rgb;
}
