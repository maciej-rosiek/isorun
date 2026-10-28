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
