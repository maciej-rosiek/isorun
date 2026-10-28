@group(3) @binding(0) var<uniform> material: Materials;
@group(3) @binding(1) var material_cells: texture_2d<f32>;
@group(3) @binding(2) var material_heat: texture_2d<f32>;

const FACET_TEXELS: f32 = 2.0;
const SWAY_BEND: f32 = 0.05;
const WET_SHEEN: f32 = 16.0;
const METAL_SHEEN: f32 = 24.0;
const CRYSTAL_SHEEN: f32 = 40.0;
const GLASS_SHEEN: f32 = 64.0;
const ICE_SHEEN: f32 = 32.0;
const SLIME_SHEEN: f32 = 20.0;

fn class_at(at: vec2<f32>) -> u32 {
    let size = textureDimensions(normal_map);
    let texel = min(vec2<u32>(max(at, vec2<f32>(0.0)) * vec2<f32>(size)), size - 1u);
    return class_of(textureLoad(normal_map, texel, 0).a);
}

fn cell_phase(world: vec3<f32>) -> f32 {
    return hash1(floor(world.x) * 61.0 + floor(world.z) * 17.0 + 4096.0) * 6.2831;
}

fn material_at(in: VertexOut, at: vec2<f32>) -> vec2<f32> {
    if (material.sway <= 0.0 || class_at(at) != CLASS_PLANT) {
        return at;
    }
    let rect = in.uv_rect;
    let phase = cell_phase(in.world);
    let clock = time() * material.sway_speed;
    let gust = sin(clock + phase) + 0.35 * sin(clock * 2.3 + phase * 1.7);
    let height = saturate1(in.local_y);
    let bend = gust * material.sway * height * height * (rect.z - rect.x) * SWAY_BEND;
    return vec2<f32>(clamp(at.x + bend, rect.x, rect.z), at.y);
}

fn sparkle(cell: vec2<f32>, view_world: vec3<f32>, seed: f32) -> f32 {
    return hash1(cell.x * 31.0 + cell.y * 977.0 + view_turn(view_world) * 131.0 + seed);
}

fn material_out(in: VertexOut, at: vec2<f32>, given: Surface, given_light: vec3<f32>, alpha: f32, face: f32, kind: u32) -> FragmentOut {
    var s = given;
    var light = given_light;
    let made_of = class_at(at);
    let ground_normal = normalize(in.normal_world);
    let shade = memory_shade(memory_cell(in.world, ground_normal));
    let seen = step(0.99, shade);
    let standing = kind == SURFACE_SPRITE || kind == SURFACE_ACTOR;
    let wall_face = !standing && abs(ground_normal.y) < 0.5;
    let view_world = world_dir(s.view_dir);
    let normal_world = world_dir(s.normal);
    let mirror = reflect(-view_world, normal_world);
    let glancing = pow(1.0 - saturate1(dot(s.normal, s.view_dir)), 3.0);
    let base = s.diffuse;
    let brightness = luminance(base);
    let clock = time();
    var reflection = vec3<f32>(0.0);
    if (!standing) {
        let wet = saturate1(wetness_at(in.world).r * material.wet_gloss);
        s.specular = s.specular + vec3<f32>(wet * 0.5);
        s.shininess = mix(s.shininess, 110.0, wet);
        reflection = reflection + (room_light(mirror) * (0.3 + 0.7 * glancing) + sheen(s, WET_SHEEN) * 0.3 * seen) * wet * material.puddles * smoothstep(0.5, 0.9, ground_normal.y);
        let heat = heat_at(in.world) * material.lava_heat;
        if (heat > 0.0) {
            let flicker = 1.0 - material.heat_flicker * noise_at(in.world.xz * 0.35 + vec2<f32>(clock * 0.21, clock * -0.17));
            light = light + material.heat_colour.rgb * heat * heat * flicker * (0.25 + brightness) * seen;
        }
    }
    switch made_of {
        case CLASS_METAL, CLASS_GOLD: {
            var hue = base / max(max(base.r, max(base.g, base.b)), 0.02);
            if (made_of == CLASS_GOLD) {
                hue = mix(hue, GOLD_TINT, 0.5);
            }
            let shine = material.metal_shine;
            s.diffuse = base * (1.0 - 0.6 * saturate1(shine));
            s.specular = hue * shine;
            s.shininess = material.metal_sharpness;
            reflection = reflection + hue * shine * (room_light(mirror) * (1.0 + glancing) + sheen(s, METAL_SHEEN) * 0.5 * seen);
        }
        case CLASS_CRYSTAL: {
            let facet = floor(at * (PAGE_SOURCE_TEXELS / FACET_TEXELS));
            let tilt = vec3<f32>(hash1(facet.x * 7.0 + facet.y * 1031.0), hash1(facet.x * 13.0 + facet.y * 517.0 + 3.0), 0.5) - 0.5;
            let facet_normal = normalize(normal_world + tilt * 0.9);
            let rect = in.uv_rect;
            let size = rect.zw - rect.xy;
            let inside = tangent_space_view(in.view_pos, normalize(in.normal_view) * face, in.uv, s.view_dir);
            let deep = rect.xy + fract((at - rect.xy) / size - inside.xy / max(inside.z, 0.3) * material.crystal_depth * 0.5) * size;
            let seen_inside = textureSampleLevel(colour_map, page_sampler, deep, 0.0).rgb;
            s.diffuse = mix(base, seen_inside * shade * in.colour, 0.3) * 1.15;
            let hue = fract(dot(facet_normal, view_world) * 2.5 + tilt.x);
            let rainbow = 0.5 + 0.5 * cos(6.2831 * (hue + vec3<f32>(0.0, 0.33, 0.67)));
            reflection = reflection + rainbow * material.crystal_colours * (0.15 + glancing) * (0.2 + brightness) * seen;
            let flash = step(0.975, sparkle(facet, view_world, 7.0)) * (0.6 + 0.4 * sin(clock * 3.0 + tilt.y * 40.0));
            var facet_surface = s;
            facet_surface.normal = normalize((frame.view * vec4<f32>(facet_normal, 0.0)).xyz);
            light = light + vec3<f32>(flash) * material.crystal_glints * seen;
            reflection = reflection + sheen(facet_surface, CRYSTAL_SHEEN) * 0.3 * material.crystal_glints * seen;
            s.specular = vec3<f32>(0.4);
            s.shininess = 160.0;
            reflection = reflection + room_light(reflect(-view_world, facet_normal)) * (0.1 + 0.5 * glancing);
        }
        case CLASS_GLASS: {
            s.diffuse = base * (1.0 - 0.4 * material.glass_tint);
            s.specular = vec3<f32>(0.9);
            s.shininess = 200.0;
            reflection = reflection + (room_light(mirror) * (0.3 + 0.7 * glancing) + sheen(s, GLASS_SHEEN) * 0.4 * seen) * (0.5 + material.glass_tint);
            if (wall_face && interleaved_gradient_noise(in.clip.xy) < material.see_through * (1.0 - 0.6 * glancing)) {
                discard;
            }
        }
        case CLASS_ICE: {
            s.diffuse = mix(base, base * ICE_TINT, 0.7);
            s.specular = vec3<f32>(material.ice_gloss * 0.8);
            s.shininess = 140.0;
            reflection = reflection + ICE_TINT * (room_light(mirror) * (0.25 + glancing) + sheen(s, ICE_SHEEN) * 0.4 * seen) * material.ice_gloss;
            let frost = step(0.99, sparkle(floor(at * PAGE_SOURCE_TEXELS), view_world, 11.0));
            light = light + (base * ICE_TINT * 0.1 + vec3<f32>(frost * 0.5)) * material.ice_gloss * seen;
        }
        case CLASS_PLANT: {
            if (kind == SURFACE_ACTOR) {
                let breath = 0.65 + 0.35 * sin(clock * 0.9 + cell_phase(in.world));
                light = light + mix(base, material.fungus_colour.rgb * brightness * 2.0, 0.6) * material.fungus_glow * breath * smoothstep(0.02, 0.2, brightness) * seen;
            }
        }
        case CLASS_MOLTEN: {
            let flicker = 1.0 + material.heat_flicker * (noise_at(in.world.xz * 0.6 + vec2<f32>(clock * 0.4, clock * -0.3)) - 0.5) * 2.0;
            light = light * flicker + material.heat_colour.rgb * brightness * material.lava_heat * 0.5 * seen;
        }
        case CLASS_ARCANE: {
            let sheen = 0.5 + 0.5 * sin(clock * 2.0 + in.world.y * 9.0 + (in.world.x + in.world.z) * 3.0);
            light = light + base * material.magic_glow * (0.4 + sheen) * seen;
        }
        case CLASS_SHADOW: {
            let veins = smoothstep(0.62, 0.7, noise_at(in.world.xz * 0.5 + vec2<f32>(clock * 0.03, clock * -0.02)));
            s.diffuse = base * 0.6;
            light = light + CORRUPTION_LIGHT * veins * material.corruption * seen;
        }
        case CLASS_SLIME: {
            s.specular = vec3<f32>(0.45);
            s.shininess = 60.0;
            reflection = reflection + (room_light(mirror) * (0.4 + glancing) + sheen(s, SLIME_SHEEN) * seen) * 0.25;
        }
        default: {}
    }
    let lit = light_surface(MODEL_CLAY, s) + hero_highlight(s) + reflection * shade + light;
    return outputs(lit, alpha, in, face, light);
}
