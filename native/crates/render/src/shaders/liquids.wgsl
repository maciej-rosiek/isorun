// The liquids (water.ts, lava.ts, starwater.ts): a fine grid over the seen cells of each liquid,
// whose points carry how much of it is there (`liquid`, rounded at the corners), how near (`near`)
// and how deep (`depth`); what moves on a liquid shows only in sight.

struct Water {
    deep: vec4<f32>,
    shallow: vec4<f32>,
    light: vec4<f32>,
    // strength, speed, opacity, gloss
    params: vec4<f32>,
};

@group(1) @binding(0) var<uniform> water: Water;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) liquid: f32,
    @location(2) near: f32,
    @location(3) depth: f32,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) view_pos: vec3<f32>,
    @location(2) liquid: f32,
    @location(3) near: f32,
    @location(4) depth: f32,
};

@vertex
fn vs_liquid(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.world = v.position;
    out.view_pos = (frame.view * vec4<f32>(v.position, 1.0)).xyz;
    out.clip = frame.view_proj * vec4<f32>(v.position, 1.0);
    out.liquid = v.liquid;
    out.near = v.near;
    out.depth = v.depth;
    return out;
}

struct FragmentOut {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) emission: vec4<f32>,
};

fn noise(p: vec2<f32>) -> f32 {
    return textureSampleLevel(noise_map, repeat_sampler, p, 0.0).r;
}

// How light a liquid's cell is, and 1 where it is in sight, 0 where only remembered.
fn sight(world: vec3<f32>) -> vec2<f32> {
    let shade = memory_shade(floor(world.xz));
    return vec2<f32>(shade, step(0.99, shade));
}

const RIPPLE_STEP: f32 = 0.15;

// The view-space normal of a liquid from the slope of its height, flat without liquid depth.
fn liquid_normal(here: f32, across: f32, down: f32, tilt: f32) -> vec3<f32> {
    let k = frame.shading3.y * (-tilt / RIPPLE_STEP);
    let world = normalize(vec3<f32>((across - here) * k, 1.0, (down - here) * k));
    return normalize((frame.view * vec4<f32>(world, 0.0)).xyz);
}

fn surface(in: VertexOut, normal: vec3<f32>, diffuse: vec3<f32>) -> Surface {
    var s: Surface;
    s.position = in.view_pos;
    s.normal = normal;
    s.view_dir = normalize(-in.view_pos);
    s.world = in.world;
    s.normal_world = vec3<f32>(0.0, 1.0, 0.0);
    s.diffuse = diffuse;
    s.specular = vec3<f32>(0.0);
    s.shininess = 40.0;
    s.rim = 0.0;
    return s;
}

const CAUSTIC_SCALE: f32 = 0.35;
const SWELL_SCALE: f32 = 0.15;
const FOAM_SCALE: f32 = 0.6;
const EDGE_SCALE: f32 = 0.9;
const DRIFT: f32 = 0.04;
const CAUSTIC_SHARPNESS: f32 = 6.0;
const SHALLOW_OPACITY: f32 = 0.6;
const DEEP_OPACITY: f32 = 0.95;
const EDGE_WOBBLE: f32 = 0.12;
const FOAM_WIDTH: f32 = 0.08;
const RIPPLE_TILT: f32 = 0.18;
const RIPPLE_SCALE: f32 = 0.45;

// Water, murky water, the bog, mud and the slime and acid films: a rounded shore, shallow to deep
// colour, drifting light lines and foam; Phong lit, with ripples.
@fragment
fn fs_water(in: VertexOut) -> FragmentOut {
    let place = in.world.xz;
    let clock = time() * water.params.y * DRIFT;
    let drift = noise(place * EDGE_SCALE + vec2<f32>(clock * 0.3, clock * -0.2));
    let ridge = fn_ridge(place, clock);
    let edge = in.liquid + (drift - 0.5) * EDGE_WOBBLE;
    let shape = smoothstep(0.47, 0.53, edge);
    let lit = sight(in.world);
    let caustics = pow(ridge, CAUSTIC_SHARPNESS) * (in.depth * -0.5 + 1.0);
    let swell = noise(place * SWELL_SCALE + vec2<f32>(clock * 0.4, clock * -0.3)) * 0.25 + 0.875;
    let foam = smoothstep_down(0.5 + FOAM_WIDTH, 0.5, edge) * smoothstep(0.3, 0.6, noise(place * FOAM_SCALE + vec2<f32>(clock * -0.5, clock * -0.8)));
    let glow = min(caustics + foam, 1.0) * water.params.x * lit.y;
    let colour = mix(mix(water.shallow.rgb, water.deep.rgb, in.depth) * swell, water.light.rgb, glow) * lit.x;
    let ripples = fn_ripples(place, clock);
    let normal = liquid_normal(ripples, fn_ripples(place + vec2<f32>(RIPPLE_STEP, 0.0), clock), fn_ripples(place + vec2<f32>(0.0, RIPPLE_STEP), clock), RIPPLE_TILT);
    var s = surface(in, normal, colour);
    s.specular = vec3<f32>(water.params.w * lit.y * frame.shading3.y);
    let alpha = max(shape * min(mix(SHALLOW_OPACITY, DEEP_OPACITY, in.depth) * water.params.z, 1.0), glow * shape);
    var out: FragmentOut;
    out.colour = vec4<f32>(light_surface(MODEL_PHONG, s), alpha);
    out.normal = vec4<f32>(0.0);
    out.emission = vec4<f32>(0.0);
    return out;
}

fn fn_ridge(place: vec2<f32>, clock: f32) -> f32 {
    let a = 1.0 - abs(noise(place * CAUSTIC_SCALE + vec2<f32>(clock, clock * 0.6)) * 2.0 - 1.0);
    let b = 1.0 - abs(noise(place * (CAUSTIC_SCALE * 1.3) + vec2<f32>(clock * -0.7, clock * 0.9)) * 2.0 - 1.0);
    return a * b;
}

fn fn_ripples(p: vec2<f32>, clock: f32) -> f32 {
    return noise(p * RIPPLE_SCALE + vec2<f32>(clock * 0.5, clock * -0.4)) + noise(p * (RIPPLE_SCALE * 2.3) + vec2<f32>(clock * -0.6, clock * 0.7)) * 0.5;
}

// ---------------------------------------------------------------- lava

struct Lava {
    crust: vec4<f32>,
    molten: vec4<f32>,
    hot: vec4<f32>,
    // glow, speed
    params: vec4<f32>,
};

@group(1) @binding(0) var<uniform> lava: Lava;

const LAVA_DRIFT: f32 = 0.02;
const PLATE_SCALE: f32 = 0.45;
const FINE_SCALE: f32 = 0.85;
const PULSE_SCALE: f32 = 0.12;
const LAVA_EDGE_WOBBLE: f32 = 0.1;
const CRACK_WIDTH: f32 = 0.08;
const FINE_CRACK_WIDTH: f32 = 0.04;
const RIM_OPACITY: f32 = 0.45;
const BRIGHTNESS: f32 = 0.9;
const MOLTEN_DEPTH: f32 = 0.12;
const PLATE_TILT: f32 = 0.5;

fn flow(scale: f32, dx: f32, dy: f32, at: vec2<f32>, clock: f32) -> f32 {
    return noise(at * scale + vec2<f32>(clock * dx, clock * dy));
}

fn crack(v: f32, width: f32) -> f32 {
    return smoothstep_down(width, 0.0, abs(v - 0.5));
}

fn cracks_at(at: vec2<f32>, clock: f32) -> f32 {
    return max(crack(flow(PLATE_SCALE, 1.0, 0.4, at, clock), CRACK_WIDTH), crack(flow(FINE_SCALE, -0.6, 0.8, at, clock), FINE_CRACK_WIDTH) * 0.45);
}

@fragment
fn fs_lava(in: VertexOut) -> FragmentOut {
    let place = in.world.xz;
    let clock = time() * lava.params.y * LAVA_DRIFT;
    let lit = sight(in.world);
    let edge = in.liquid + (flow(EDGE_SCALE, 0.2, -0.3, place, clock) - 0.5) * LAVA_EDGE_WOBBLE;
    let shape = smoothstep(0.47, 0.53, edge);
    let view = -(frame.inv_view * vec4<f32>(0.0, 0.0, 1.0, 0.0)).xyz;
    let deep = place + view.xz / max(-view.y, 0.2) * frame.shading3.y * MOLTEN_DEPTH;
    let pulse = flow(PULSE_SCALE, 0.3, 0.2, place, clock) * 0.6 + 0.7;
    let cooling = smoothstep(0.5, 0.8, edge);
    let heat = clamp((cracks_at(place, clock) * pulse + in.depth * 0.12) * (cooling * 0.8 + 0.2), 0.0, 1.0);
    let below = clamp((max(cracks_at(deep, clock), cracks_at(place, clock)) * pulse + in.depth * 0.12) * (cooling * 0.8 + 0.2), 0.0, 1.0);
    let melt = mix(lava.molten.rgb, lava.hot.rgb, below * below);
    let open = smoothstep(0.15, 0.6, heat);
    let rim = pow(smoothstep(0.0, 0.5, in.near), 2.0) * (1.0 - shape) * lit.y;
    let crust = lava.crust.rgb * (flow(PULSE_SCALE * 4.0, -0.2, 0.1, place, clock) * 0.7 + 0.65);
    let here = 1.0 - cracks_at(place, clock);
    let normal = liquid_normal(here, 1.0 - cracks_at(place + vec2<f32>(RIPPLE_STEP, 0.0), clock), 1.0 - cracks_at(place + vec2<f32>(0.0, RIPPLE_STEP), clock), PLATE_TILT);
    let s = surface(in, normal, crust * (1.0 - open) * lit.x);
    let emissive = (melt * BRIGHTNESS * open + lava.molten.rgb * rim) * lit.x;
    var out: FragmentOut;
    out.colour = vec4<f32>(lit_surface(MODEL_LAMBERT, s, emissive), max(shape, rim * RIM_OPACITY));
    out.normal = vec4<f32>(0.0);
    out.emission = vec4<f32>(melt * below * shape * lit.y * lava.params.x, 1.0);
    return out;
}

// ---------------------------------------------------------------- starwater

const STAR_DEEP: vec3<f32> = vec3<f32>(0.012, 0.016, 0.05);
const NEBULA: vec3<f32> = vec3<f32>(0.065, 0.026, 0.125);
const STAR: vec3<f32> = vec3<f32>(0.75, 0.85, 1.0);
const STAR_DRIFT: f32 = 0.015;

@fragment
fn fs_starwater(in: VertexOut) -> FragmentOut {
    let place = in.world.xz;
    let clock = time() * STAR_DRIFT;
    let lit = sight(in.world);
    let edge = in.liquid + (noise(place * 0.9 + vec2<f32>(clock * 0.3, clock * -0.2)) - 0.5) * 0.1;
    let shape = smoothstep(0.47, 0.53, edge);
    let view = -(frame.inv_view * vec4<f32>(0.0, 0.0, 1.0, 0.0)).xyz;
    let slant = view.xz / max(-view.y, 0.2) * frame.shading3.y;
    let sky = mix(STAR_DEEP, NEBULA, smoothstep(0.4, 0.75, noise((place + slant * 3.2) * 0.18 + vec2<f32>(clock, clock * -0.6))));
    let twinkle = smoothstep(0.3, 0.75, noise(place * 0.9 + vec2<f32>(clock * 9.0, clock * 6.0)));
    var stars = 0.0;
    let depths = array<vec3<f32>, 3>(vec3<f32>(0.3, 1.0, 1.0), vec3<f32>(1.1, 1.7, 0.6), vec3<f32>(2.4, 2.6, 0.35));
    for (var i = 0; i < 3; i = i + 1) {
        let layer = depths[i];
        let p = place + slant * layer.x;
        let points = smoothstep(0.42, 0.5, noise(p * (2.6 * layer.y)) * noise(p * (1.7 * layer.y) + 0.37));
        stars = stars + points * layer.z;
    }
    let star = min(stars, 1.0) * twinkle * lit.y * smoothstep(0.55, 0.75, edge);
    var out: FragmentOut;
    let sky_surface = surface(in, frame.up.xyz, sky * lit.x);
    out.colour = vec4<f32>(cell_memory_look(sky_surface, mix(sky, STAR * 2.5, star) * lit.x), shape * 0.95);
    out.normal = vec4<f32>(0.0);
    out.emission = vec4<f32>(STAR * star * shape * 1.5, 1.0);
    return out;
}
