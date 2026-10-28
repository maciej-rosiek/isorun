struct Materials {
    hero_position: vec4<f32>,
    hero_colour: vec4<f32>,
    room_colour: vec4<f32>,
    heat_colour: vec4<f32>,
    fungus_colour: vec4<f32>,
    hero_reach: f32,
    wall_height: f32,
    metal_shine: f32,
    metal_sharpness: f32,
    crystal_glints: f32,
    crystal_colours: f32,
    crystal_depth: f32,
    see_through: f32,
    glass_tint: f32,
    ice_gloss: f32,
    wet_gloss: f32,
    puddles: f32,
    water_reflection: f32,
    sway: f32,
    sway_speed: f32,
    fungus_glow: f32,
    magic_glow: f32,
    lava_heat: f32,
    heat_flicker: f32,
    corruption: f32,
};

struct MaterialIn {
    made_of: u32,
    kind: u32,
    world: vec3<f32>,
    ground_normal: vec3<f32>,
    shade: f32,
    facet: vec2<f32>,
    grain: vec2<f32>,
    inside: vec3<f32>,
    pixel: vec2<f32>,
};

struct MaterialLit {
    colour: vec3<f32>,
    light: vec3<f32>,
};

const CLASS_PLAIN: u32 = 0u;
const CLASS_STONE: u32 = 1u;
const CLASS_METAL: u32 = 2u;
const CLASS_GOLD: u32 = 3u;
const CLASS_CRYSTAL: u32 = 4u;
const CLASS_GLASS: u32 = 5u;
const CLASS_ICE: u32 = 6u;
const CLASS_PLANT: u32 = 7u;
const CLASS_MOLTEN: u32 = 8u;
const CLASS_ARCANE: u32 = 9u;
const CLASS_SHADOW: u32 = 10u;
const CLASS_SLIME: u32 = 11u;
const CLASS_STEPS: f32 = 15.0;

const SWAY_BEND: f32 = 0.05;
const WET_SHEEN: f32 = 16.0;
const METAL_SHEEN: f32 = 24.0;
const CRYSTAL_SHEEN: f32 = 40.0;
const GLASS_SHEEN: f32 = 64.0;
const ICE_SHEEN: f32 = 32.0;
const SLIME_SHEEN: f32 = 20.0;
const MIRROR_STEPS: i32 = 10;
const MIRROR_REACH: f32 = 3.0;
const MIRROR_FLAME: f32 = 0.15;
const ROOM_AMBIENT: f32 = 0.05;
const ROOM_BASE: f32 = 0.3;
const ROOM_WINDOWS: f32 = 6.0;
const GOLD_TINT: vec3<f32> = vec3<f32>(1.0, 0.72, 0.32);
const ICE_TINT: vec3<f32> = vec3<f32>(0.72, 0.9, 1.15);
const CORRUPTION_LIGHT: vec3<f32> = vec3<f32>(0.42, 0.08, 0.6);

fn class_of(alpha: f32) -> u32 {
    return u32(round(alpha * CLASS_STEPS));
}

fn page_texel(at: vec2<f32>, size: vec2<u32>) -> vec2<u32> {
    return min(vec2<u32>(max(at, vec2<f32>(0.0)) * vec2<f32>(size)), size - 1u);
}

fn world_dir(v: vec3<f32>) -> vec3<f32> {
    return normalize((frame.inv_view * vec4<f32>(v, 0.0)).xyz);
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn grid_texel(cell: vec2<f32>) -> vec2<i32> {
    let side = i32(MEMORY_CELLS);
    return ((vec2<i32>(cell) % side) + side) % side;
}

fn view_turn(view_world: vec3<f32>) -> f32 {
    return floor(atan2(view_world.z, view_world.x) * 9.0 + view_world.y * 7.0) + 64.0;
}

fn room_light(direction: vec3<f32>) -> vec3<f32> {
    let around = atan2(direction.z, direction.x);
    let horizon = smoothstep(-0.2, 0.2, direction.y) * (1.0 - smoothstep(0.45, 0.95, direction.y));
    let windows = pow(saturate1(sin(around * 3.0 + 0.6) * 0.5 + 0.5), 10.0) + 0.6 * pow(saturate1(sin(around * 7.0 - 1.3) * 0.5 + 0.5), 24.0);
    let ambient = mix(frame.ground.rgb, frame.sky.rgb, saturate1(direction.y * 0.5 + 0.5)) / PI;
    return ambient * ROOM_AMBIENT + material.room_colour.rgb * (ROOM_BASE + windows * horizon * ROOM_WINDOWS);
}

fn hero_highlight(s: Surface) -> vec3<f32> {
    let to_hero = material.hero_position.xyz - s.position;
    let far = length(to_hero);
    if (material.hero_reach <= 0.0 || far >= material.hero_reach) {
        return vec3<f32>(0.0);
    }
    let towards = to_hero / max(far, 0.001);
    let halfway = normalize(towards + s.view_dir);
    let fade = pow(1.0 - far / material.hero_reach, 2.0);
    return material.hero_colour.rgb * fade * pow(saturate1(dot(s.normal, halfway)), s.shininess) * saturate1(dot(s.normal, towards)) * s.specular;
}

fn sheen(s: Surface, exponent: f32) -> vec3<f32> {
    let mirror = reflect(-s.view_dir, s.normal);
    var out = vec3<f32>(0.0);
    let to_hero = material.hero_position.xyz - s.position;
    let far = length(to_hero);
    if (material.hero_reach > 0.0 && far < material.hero_reach) {
        out = out + material.hero_colour.rgb * pow(1.0 - far / material.hero_reach, 2.0) * pow(saturate1(dot(mirror, to_hero / max(far, 0.001))), exponent);
    }
    let baked = textureSampleLevel(torch_light_map, repeat_sampler, (s.world.xz + s.normal_world.xz * (0.5 / TORCH_LIGHT_TEXELS)) / MEMORY_CELLS, 0.0);
    if (baked.a > 0.0001) {
        let across = baked.rb / baked.a;
        let to_torch = (frame.view * vec4<f32>(normalize(vec3<f32>(across.x, frame.torch_light.w - s.world.y, across.y)), 0.0)).xyz;
        out = out + frame.torch_light.rgb * limited_torch_light(baked.a) * pow(saturate1(dot(mirror, to_torch)), exponent);
    }
    return out;
}

fn heat_at(world: vec3<f32>) -> f32 {
    return textureSampleLevel(material_heat, repeat_sampler, world.xz / MEMORY_CELLS, 0.0).r;
}

fn mirrored_walls(start: vec3<f32>, ray: vec3<f32>) -> vec4<f32> {
    let across = length(ray.xz);
    if (ray.y <= 0.02 || across <= 0.001) {
        return vec4<f32>(0.0);
    }
    let rise = ray.y / across;
    let reach = clamp((material.wall_height - start.y) / rise, 0.0, MIRROR_REACH);
    let way = ray.xz / across;
    for (var i = 0; i < MIRROR_STEPS; i = i + 1) {
        let along = (f32(i) + 0.5) / f32(MIRROR_STEPS) * reach;
        let at = start.xz + way * along;
        let wall = textureLoad(material_cells, grid_texel(floor(at)), 0);
        if (wall.a > 0.25) {
            let height = start.y + rise * along;
            let torch = textureSampleLevel(torch_light_map, repeat_sampler, (at - way * 0.25) / MEMORY_CELLS, 0.0).a;
            let lit = wall.rgb * ((frame.sky.rgb + frame.ground.rgb) * 0.5 + frame.torch_light.rgb * limited_torch_light(torch)) / PI;
            var flame = 0.0;
            if (wall.a > 0.75) {
                let off = abs(fract(at) - 0.5);
                flame = smoothstep(0.3, 0.0, abs(height - frame.torch_light.w)) * smoothstep(0.3, 0.05, min(off.x, off.y));
            }
            return vec4<f32>(lit + frame.torch_light.rgb * flame * MIRROR_FLAME, 1.0);
        }
    }
    return vec4<f32>(0.0);
}

fn cell_phase(world: vec3<f32>) -> f32 {
    return hash1(floor(world.x) * 61.0 + floor(world.z) * 17.0 + 4096.0) * 6.2831;
}

fn sway_at(world: vec3<f32>, height: f32) -> f32 {
    let phase = cell_phase(world);
    let clock = time() * material.sway_speed;
    let gust = sin(clock + phase) + 0.35 * sin(clock * 2.3 + phase * 1.7);
    return gust * material.sway * height * height * SWAY_BEND;
}

fn sparkle(cell: vec2<f32>, view_world: vec3<f32>, seed: f32) -> f32 {
    return hash1(cell.x * 31.0 + cell.y * 977.0 + view_turn(view_world) * 131.0 + seed);
}

fn material_lit(m: MaterialIn, given: Surface, given_light: vec3<f32>) -> MaterialLit {
    var s = given;
    var light = given_light;
    let made_of = m.made_of;
    let kind = m.kind;
    let ground_normal = m.ground_normal;
    let shade = m.shade;
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
        let wet = saturate1(wetness_at(m.world).r * material.wet_gloss);
        s.specular = s.specular + vec3<f32>(wet * 0.5);
        s.shininess = mix(s.shininess, 110.0, wet);
        reflection = reflection + (room_light(mirror) * (0.3 + 0.7 * glancing) + sheen(s, WET_SHEEN) * 0.3 * seen) * wet * material.puddles * smoothstep(0.5, 0.9, ground_normal.y);
        let heat = heat_at(m.world) * material.lava_heat;
        if (heat > 0.0) {
            let flicker = 1.0 - material.heat_flicker * noise_at(m.world.xz * 0.35 + vec2<f32>(clock * 0.21, clock * -0.17));
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
            let facet = m.facet;
            let tilt = vec3<f32>(hash1(facet.x * 7.0 + facet.y * 1031.0), hash1(facet.x * 13.0 + facet.y * 517.0 + 3.0), 0.5) - 0.5;
            let facet_normal = normalize(normal_world + tilt * 0.9);
            s.diffuse = mix(base, m.inside, 0.3) * 1.15;
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
            if (wall_face && interleaved_gradient_noise(m.pixel) < material.see_through * (1.0 - 0.6 * glancing)) {
                discard;
            }
        }
        case CLASS_ICE: {
            s.diffuse = mix(base, base * ICE_TINT, 0.7);
            s.specular = vec3<f32>(material.ice_gloss * 0.8);
            s.shininess = 140.0;
            reflection = reflection + ICE_TINT * (room_light(mirror) * (0.25 + glancing) + sheen(s, ICE_SHEEN) * 0.4 * seen) * material.ice_gloss;
            let frost = step(0.99, sparkle(m.grain, view_world, 11.0));
            light = light + (base * ICE_TINT * 0.1 + vec3<f32>(frost * 0.5)) * material.ice_gloss * seen;
        }
        case CLASS_PLANT: {
            if (kind == SURFACE_ACTOR) {
                let breath = 0.65 + 0.35 * sin(clock * 0.9 + cell_phase(m.world));
                light = light + mix(base, material.fungus_colour.rgb * brightness * 2.0, 0.6) * material.fungus_glow * breath * smoothstep(0.02, 0.2, brightness) * seen;
            }
        }
        case CLASS_MOLTEN: {
            let flicker = 1.0 + material.heat_flicker * (noise_at(m.world.xz * 0.6 + vec2<f32>(clock * 0.4, clock * -0.3)) - 0.5) * 2.0;
            light = light * flicker + material.heat_colour.rgb * brightness * material.lava_heat * 0.5 * seen;
        }
        case CLASS_ARCANE: {
            let sheen = 0.5 + 0.5 * sin(clock * 2.0 + m.world.y * 9.0 + (m.world.x + m.world.z) * 3.0);
            light = light + base * material.magic_glow * (0.4 + sheen) * seen;
        }
        case CLASS_SHADOW: {
            let veins = smoothstep(0.62, 0.7, noise_at(m.world.xz * 0.5 + vec2<f32>(clock * 0.03, clock * -0.02)));
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
    var out: MaterialLit;
    out.colour = lit_surface(MODEL_CLAY, s, hero_highlight(s) + reflection * shade + light);
    out.light = light;
    return out;
}
