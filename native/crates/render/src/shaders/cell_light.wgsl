override SIGHT_LIGHT: bool = false;
const CELL_LIGHT_LEAST_WEIGHT: f32 = 0.0001;
const CELL_SIGHT_EDGE_LOW: f32 = 0.25;
const CELL_SIGHT_EDGE_HIGH: f32 = 0.75;
const CELL_TOP_HEIGHT: f32 = 0.25;
const CELL_MEMORY_SIDE: f32 = 0.6;
const CELL_MEMORY_TOP: f32 = 0.45;
const CELL_LUMINANCE: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

struct CellLightFrame {
    hero: vec4<f32>,
    hero_colour: vec4<f32>,
    memory: vec4<f32>,
    fill: vec4<f32>,
    halo: vec4<f32>,
};

@group(0) @binding(11) var<uniform> cell_light_frame: CellLightFrame;
@group(0) @binding(12) var cell_sight_map: texture_2d<f32>;

fn cell_on_top(s: Surface) -> bool {
    return s.all_round < 0.5 && s.normal_world.y > 0.5 && s.world.y > CELL_TOP_HEIGHT;
}

fn cell_light_point(s: Surface) -> vec2<f32> {
    if (s.all_round > 0.5) {
        return s.world.xz / MEMORY_CELLS;
    }
    if (cell_on_top(s)) {
        return (floor(s.world.xz) + 0.5) / MEMORY_CELLS;
    }
    return (s.world.xz + s.normal_world.xz * 0.5) / MEMORY_CELLS;
}

fn cell_light_unweighted(v: vec4<f32>) -> vec3<f32> {
    if (v.a <= CELL_LIGHT_LEAST_WEIGHT) {
        return vec3<f32>(0.0);
    }
    return v.rgb / v.a;
}

fn cell_own_point(s: Surface) -> vec2<f32> {
    return (floor(s.world.xz - s.normal_world.xz * 0.02) + 0.5) / MEMORY_CELLS;
}

fn cell_sight_at(s: Surface, at: vec2<f32>) -> vec3<f32> {
    var v = textureSampleLevel(cell_sight_map, repeat_sampler, at, 0.0);
    if (v.a <= CELL_LIGHT_LEAST_WEIGHT) {
        v = textureSampleLevel(cell_sight_map, repeat_sampler, cell_own_point(s), 0.0);
    }
    return cell_light_unweighted(v);
}

fn cell_fill_kept() -> f32 {
    if (SIGHT_LIGHT) {
        return cell_light_frame.fill.x;
    }
    return 1.0;
}

fn cell_spread_light(s: Surface, colour: vec3<f32>) -> vec3<f32> {
    if (s.all_round > 0.5) {
        return colour * ALL_ROUND_SHADE * s.diffuse / PI;
    }
    return colour * s.diffuse / PI;
}

fn cell_hero_flicker() -> f32 {
    let t = time();
    return 1.0 + cell_light_frame.fill.y * (sin(t * 7.3) * 0.6 + sin(t * 13.1 + 1.7) * 0.4);
}

fn cell_hero_light(model: u32, s: Surface, seen: f32) -> vec3<f32> {
    let to_hero = cell_light_frame.hero.xyz - s.world;
    let d = max(length(to_hero), 0.001);
    let fade = mix(1.0, cell_light_frame.hero_colour.w, smoothstep(0.0, cell_light_frame.hero.w, d));
    let colour = cell_light_frame.hero_colour.rgb * fade * seen * cell_hero_flicker() * on_lit_side(s, to_hero);
    let towards = (frame.view * vec4<f32>(to_hero / d, 0.0)).xyz;
    if (cell_on_top(s)) {
        return side_light(s, towards, colour);
    }
    return direct(model, s, towards, colour, s.all_round > 0.5);
}

fn cell_remembered(s: Surface) -> vec3<f32> {
    var shape = mix(CELL_MEMORY_SIDE, 1.0, saturate1(dot(s.normal, frame.up.xyz)));
    if (cell_on_top(s)) {
        shape = CELL_MEMORY_TOP;
    }
    return dot(s.diffuse, CELL_LUMINANCE) * shape * cell_light_frame.memory.rgb;
}

fn cell_seen(sight: vec3<f32>) -> f32 {
    return smoothstep(CELL_SIGHT_EDGE_LOW, CELL_SIGHT_EDGE_HIGH, sight.r);
}

fn cell_memory_look(s: Surface, colour: vec3<f32>) -> vec3<f32> {
    if (!SIGHT_LIGHT) {
        return colour;
    }
    return mix(cell_remembered(s), colour, cell_seen(cell_sight_at(s, cell_light_point(s))));
}

fn cell_light_surface(model: u32, s: Surface, lit: vec3<f32>, own: vec3<f32>) -> vec3<f32> {
    if (!SIGHT_LIGHT) {
        return lit + own;
    }
    let sight = cell_sight_at(s, cell_light_point(s));
    let seen = cell_seen(sight);
    var out = lit + cell_hero_light(model, s, seen) + cell_spread_light(s, cell_light_frame.halo.rgb * sight.g);
    out = out * (1.0 - saturate1(sight.b) * cell_light_frame.halo.w);
    return mix(cell_remembered(s), out + own, seen);
}
