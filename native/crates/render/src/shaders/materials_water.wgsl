@group(2) @binding(0) var<uniform> material: Materials;
@group(2) @binding(1) var material_cells: texture_2d<f32>;
@group(2) @binding(2) var material_heat: texture_2d<f32>;

const CLEAR_WATER_GLOSS: f32 = 0.45;
const WATER_MIRROR: f32 = 0.45;
const WATER_MIRROR_MOST: f32 = 0.85;
const WATER_SHEEN: f32 = 48.0;

fn water_colour(in: VertexOut, s: Surface, alpha: f32) -> vec4<f32> {
    let lit = sight(in.world);
    let clearness = water.params.w / CLEAR_WATER_GLOSS;
    let ray = reflect(-world_dir(s.view_dir), world_dir(s.normal));
    let walls = mirrored_walls(in.world, ray);
    let mirrored = mix(room_light(ray), walls.rgb, walls.a) + sheen(s, WATER_SHEEN) * clearness * lit.y;
    let glancing = pow(1.0 - saturate1(dot(s.normal, s.view_dir)), 3.0);
    let strength = clamp(material.water_reflection * clearness * (WATER_MIRROR + (1.0 - WATER_MIRROR) * glancing), 0.0, WATER_MIRROR_MOST) * lit.x;
    let colour = mix(lit_surface(MODEL_PHONG, s, hero_highlight(s)), cell_memory_look(s, mirrored), strength);
    return vec4<f32>(colour, alpha + (1.0 - alpha) * strength * saturate1(alpha * 2.0));
}
