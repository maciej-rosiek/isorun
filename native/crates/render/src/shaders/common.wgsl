// Tint (the browsers' WebGPU) checks that derivatives and implicit-lod samples run in uniform
// control flow more strictly than naga; the shaders take them after a discard or in a branch on
// purpose, where the neighbours' values are close enough.
diagnostic(off, derivative_uniformity);

// Shared by the world's shaders: the frame's uniforms, the lights, the memory mask,
// noise. Lighting is in view space with the formulas of three.js's nodes (Phong, Lambert, the
// hemisphere light, distance attenuation), which the web build used, so the look carries over.

const PI: f32 = 3.141592653589793;
const MAX_LIGHTS: u32 = 9u;
const MEMORY_CELLS: f32 = 128.0;

struct Light {
    // view-space position, w = cutoff distance
    position: vec4<f32>,
    // colour times intensity, w = decay
    colour: vec4<f32>,
    // world-space position, w unused
    world: vec4<f32>,
};

struct Frame {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    inv_view: mat4x4<f32>,
    // world-space camera position, w = 1 for perspective
    camera: vec4<f32>,
    sky: vec4<f32>,
    ground: vec4<f32>,
    // wrap, bands, relief, depth
    shading0: vec4<f32>,
    // sprites, gloss, rim, cavity
    shading1: vec4<f32>,
    // variation, wetness, glow, sparkle
    shading2: vec4<f32>,
    // depth steps, liquid depth, statuses shown, time
    shading3: vec4<f32>,
    // width, height, light count, unused
    viewport: vec4<f32>,
    // view-space up, w unused
    up: vec4<f32>,
    torch_light: vec4<f32>,
    lights: array<Light, MAX_LIGHTS>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(4) var memory_map: texture_2d<f32>;
@group(0) @binding(5) var wet_map: texture_2d<f32>;
@group(0) @binding(6) var nearest_sampler: sampler;
@group(0) @binding(7) var linear_sampler: sampler;
@group(0) @binding(8) var noise_map: texture_2d<f32>;
@group(0) @binding(9) var repeat_sampler: sampler;
@group(0) @binding(10) var torch_light_map: texture_2d<f32>;

fn time() -> f32 {
    return frame.shading3.w;
}

// three's WGSL builder writes dFdy as -dpdy: the derivatives run up the screen, as in GLSL.
fn dfdy2(v: vec2<f32>) -> vec2<f32> {
    return -dpdy(v);
}
fn dfdy3(v: vec3<f32>) -> vec3<f32> {
    return -dpdy(v);
}

// smoothstep from 1 at `high` down to 0 at `low` (`low` > `high`): GLSL's and WGSL's smoothstep
// leave a reversed edge undefined, which three's nodes lean on; this is what it computes on the web.
fn smoothstep_down(low: f32, high: f32, x: f32) -> f32 {
    return 1.0 - smoothstep(high, low, x);
}

fn saturate1(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

// three's `hash`: a PCG hash of the seed as an unsigned integer.
fn hash1(seed: f32) -> f32 {
    let state = u32(max(seed, 0.0)) * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    let result = (word >> 22u) ^ word;
    return f32(result) * (1.0 / 4294967296.0);
}

fn interleaved_gradient_noise(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

// MaterialX's Perlin noise (three's `mx_noise_float`), about -1..1.
fn mx_rot(x: u32, k: u32) -> u32 {
    return (x << k) | (x >> (32u - k));
}
fn mx_hash3(x: i32, y: i32, z: i32) -> u32 {
    let len = 3u;
    var a = 0xdeadbeefu + (len << 2u) + 13u;
    var b = a;
    var c = a;
    a = a + u32(x);
    b = b + u32(y);
    c = c + u32(z);
    // lookup3 final mix
    c = c ^ b; c = c - mx_rot(b, 14u);
    a = a ^ c; a = a - mx_rot(c, 11u);
    b = b ^ a; b = b - mx_rot(a, 25u);
    c = c ^ b; c = c - mx_rot(b, 16u);
    a = a ^ c; a = a - mx_rot(c, 4u);
    b = b ^ a; b = b - mx_rot(a, 14u);
    c = c ^ b; c = c - mx_rot(b, 24u);
    return c;
}
fn mx_fade(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}
fn mx_grad3(hash: u32, x: f32, y: f32, z: f32) -> f32 {
    let h = hash & 15u;
    var u = y;
    if (h < 8u) { u = x; }
    var v = z;
    if (h < 4u) { v = y; } else if (h == 12u || h == 14u) { v = x; }
    var su = u;
    if ((h & 1u) != 0u) { su = -u; }
    var sv = v;
    if ((h & 2u) != 0u) { sv = -v; }
    return su + sv;
}
fn mx_noise_float(p: vec3<f32>) -> f32 {
    let fl = floor(p);
    let f = p - fl;
    let i = vec3<i32>(fl);
    let u = mx_fade(f.x);
    let v = mx_fade(f.y);
    let w = mx_fade(f.z);
    let n000 = mx_grad3(mx_hash3(i.x, i.y, i.z), f.x, f.y, f.z);
    let n100 = mx_grad3(mx_hash3(i.x + 1, i.y, i.z), f.x - 1.0, f.y, f.z);
    let n010 = mx_grad3(mx_hash3(i.x, i.y + 1, i.z), f.x, f.y - 1.0, f.z);
    let n110 = mx_grad3(mx_hash3(i.x + 1, i.y + 1, i.z), f.x - 1.0, f.y - 1.0, f.z);
    let n001 = mx_grad3(mx_hash3(i.x, i.y, i.z + 1), f.x, f.y, f.z - 1.0);
    let n101 = mx_grad3(mx_hash3(i.x + 1, i.y, i.z + 1), f.x - 1.0, f.y, f.z - 1.0);
    let n011 = mx_grad3(mx_hash3(i.x, i.y + 1, i.z + 1), f.x, f.y - 1.0, f.z - 1.0);
    let n111 = mx_grad3(mx_hash3(i.x + 1, i.y + 1, i.z + 1), f.x - 1.0, f.y - 1.0, f.z - 1.0);
    let r = mix(mix(mix(n000, n100, u), mix(n010, n110, u), v), mix(mix(n001, n101, u), mix(n011, n111, u), v), w);
    return 0.982 * r;
}

fn noise_at(p: vec2<f32>) -> f32 {
    return textureSampleLevel(noise_map, repeat_sampler, p, 0.0).r;
}

// The surface's cell, in cells: a wall's face lies on the edge of its cell, so look a little behind.
fn memory_cell(world: vec3<f32>, normal_world: vec3<f32>) -> vec2<f32> {
    return floor(world.xz - normal_world.xz * 0.02);
}

// How light the surface's cell is: 1 in sight, darker when only remembered.
fn memory_shade(cell: vec2<f32>) -> f32 {
    return textureSampleLevel(memory_map, nearest_sampler, fract((cell + 0.5) / MEMORY_CELLS), 0.0).r;
}

// How wet the ground is here (r) and how deep under see-through water (g), smooth between cells.
fn wetness_at(world: vec3<f32>) -> vec2<f32> {
    return textureSampleLevel(wet_map, repeat_sampler, world.xz / MEMORY_CELLS, 0.0).rg;
}

// three's getDistanceAttenuation.
fn distance_attenuation(d: f32, cutoff: f32, decay: f32) -> f32 {
    let falloff = 1.0 / max(pow(d, decay), 0.01);
    if (cutoff > 0.0) {
        let c = saturate1(1.0 - pow(d / cutoff, 4.0));
        return falloff * c * c;
    }
    return falloff;
}

struct Surface {
    // view space
    position: vec3<f32>,
    normal: vec3<f32>,
    view_dir: vec3<f32>,
    // world space
    world: vec3<f32>,
    normal_world: vec3<f32>,
    diffuse: vec3<f32>,
    specular: vec3<f32>,
    shininess: f32,
    rim: f32,
    // 1: lit evenly from any side (a sprite, an actor); a float, as a bool here crashed the shader
    // compiler of Mali's Vulkan driver (a Pixel 8 Pro's Mali-G715) in vkCreateGraphicsPipelines
    all_round: f32,
};

// ClayLightingModel.direct: wrapped, banded diffuse, a Blinn-Phong highlight, a rim light.
fn clay_direct(s: Surface, dir: vec3<f32>, colour: vec3<f32>, all_round: bool) -> vec3<f32> {
    if (all_round) {
        return colour * ALL_ROUND_SHADE * s.diffuse / PI;
    }
    let wrap = frame.shading0.x;
    let bands = frame.shading0.y;
    let facing = dot(s.normal, dir);
    let wrapped = saturate1((facing + wrap) / (wrap + 1.0));
    var shade = wrapped;
    if (bands > 0.5) {
        shade = ceil(wrapped * bands) / max(bands, 1.0);
    }
    var out = colour * shade * s.diffuse / PI;
    let halfway = normalize(dir + s.view_dir);
    let highlight = pow(saturate1(dot(s.normal, halfway)), s.shininess) * saturate1(dot(s.normal, dir));
    out = out + colour * highlight * s.specular;
    let edge = pow(1.0 - saturate1(dot(s.normal, s.view_dir)), 3.0);
    let behind = mix(0.35, 1.0, saturate1(-dot(dir, s.view_dir)));
    out = out + colour * edge * behind * s.rim * frame.shading1.z * mix(vec3<f32>(1.0), s.diffuse, 0.5);
    return out;
}

// PhongLightingModel.direct: Lambert diffuse and three's BRDF_BlinnPhong.
fn phong_direct(s: Surface, dir: vec3<f32>, colour: vec3<f32>) -> vec3<f32> {
    let irradiance = saturate1(dot(s.normal, dir)) * colour;
    let halfway = normalize(dir + s.view_dir);
    let dot_nh = saturate1(dot(s.normal, halfway));
    let dot_vh = saturate1(dot(s.view_dir, halfway));
    let fresnel = s.specular + (vec3<f32>(1.0) - s.specular) * pow(1.0 - dot_vh, 5.0);
    let d = (1.0 / PI) * (s.shininess * 0.5 + 1.0) * pow(dot_nh, s.shininess);
    return irradiance * s.diffuse / PI + irradiance * fresnel * 0.25 * d;
}

fn lambert_direct(s: Surface, dir: vec3<f32>, colour: vec3<f32>) -> vec3<f32> {
    return saturate1(dot(s.normal, dir)) * colour * s.diffuse / PI;
}

const MODEL_CLAY: u32 = 0u;
const MODEL_PHONG: u32 = 1u;
const MODEL_LAMBERT: u32 = 2u;

const ALL_ROUND_SHADE: f32 = 0.35;

fn direct(model: u32, s: Surface, dir: vec3<f32>, colour: vec3<f32>, all_round: bool) -> vec3<f32> {
    if (model == MODEL_PHONG) {
        return phong_direct(s, dir, colour);
    }
    if (model == MODEL_LAMBERT) {
        return lambert_direct(s, dir, colour);
    }
    return clay_direct(s, dir, colour, all_round);
}

const TORCH_LIGHT_TEXELS: f32 = 4.0;
const TORCH_LIGHT_KNEE: f32 = 0.45;
const TORCH_LIGHT_CEILING: f32 = 0.9;

fn limited_torch_light(amount: f32) -> f32 {
    if (amount <= TORCH_LIGHT_KNEE) {
        return amount;
    }
    let room = TORCH_LIGHT_CEILING - TORCH_LIGHT_KNEE;
    return TORCH_LIGHT_KNEE + room * (1.0 - exp(-(amount - TORCH_LIGHT_KNEE) / room));
}

fn on_lit_side(s: Surface, to_light_world: vec3<f32>) -> f32 {
    if (s.all_round > 0.5) {
        return 1.0;
    }
    return smoothstep(-0.05, 0.1, dot(s.normal_world, normalize(to_light_world)));
}

fn baked_torch_light(model: u32, s: Surface) -> vec3<f32> {
    let at = s.world.xz + s.normal_world.xz * (0.5 / TORCH_LIGHT_TEXELS);
    let baked = textureSampleLevel(torch_light_map, repeat_sampler, at / MEMORY_CELLS, 0.0);
    if (baked.a <= 0.0001) {
        return vec3<f32>(0.0);
    }
    let across = baked.rb / baked.a;
    let to_torch = vec3<f32>(across.x, frame.torch_light.w - s.world.y, across.y);
    let towards = (frame.view * vec4<f32>(normalize(to_torch), 0.0)).xyz;
    return direct(model, s, towards, frame.torch_light.rgb * limited_torch_light(baked.a) * on_lit_side(s, to_torch), s.all_round > 0.5);
}

// Every light on the surface: the hemisphere light, the torches and the point lights.
fn light_surface(model: u32, s: Surface) -> vec3<f32> {
    let weight = dot(s.normal, frame.up.xyz) * 0.5 + 0.5;
    let irradiance = mix(frame.ground.rgb, frame.sky.rgb, weight);
    var out = irradiance * s.diffuse / PI;
    out = out + baked_torch_light(model, s);
    let count = u32(frame.viewport.z);
    for (var i = 0u; i < MAX_LIGHTS; i = i + 1u) {
        if (i >= count) {
            break;
        }
        let light = frame.lights[i];
        if (light.colour.r + light.colour.g + light.colour.b <= 0.0) {
            continue;
        }
        let to_light = light.position.xyz - s.position;
        let d = length(to_light);
        if (light.position.w > 0.0 && d >= light.position.w) {
            continue;
        }
        let colour = light.colour.rgb * distance_attenuation(d, light.position.w, light.colour.w) * on_lit_side(s, light.world.xyz - s.world);
        out = out + direct(model, s, to_light / d, colour, s.all_round > 0.5);
    }
    return out;
}

// three's perturbNormal2Arb: a tangent-space normal onto the surface, with the frame from derivatives.
fn perturb_normal(position: vec3<f32>, normal: vec3<f32>, uv: vec2<f32>, map_n: vec3<f32>, face: f32) -> vec3<f32> {
    let q0 = dpdx(position);
    let q1 = dfdy3(position);
    let st0 = dpdx(uv);
    let st1 = dfdy2(uv);
    let q1perp = cross(q1, normal);
    let q0perp = cross(normal, q0);
    let t = q1perp * st0.x + q0perp * st1.x;
    let b = q1perp * st0.y + q0perp * st1.y;
    let det = max(dot(t, t), dot(b, b));
    var scale = 0.0;
    if (det > 0.0) {
        scale = face * inverseSqrt(det);
    }
    return normalize(t * (map_n.x * scale) + b * (map_n.y * scale) + normal * map_n.z);
}

// The direction to the camera in the tile's texture space (along u, along v, out of the surface).
fn tangent_space_view(position: vec3<f32>, surface: vec3<f32>, uv: vec2<f32>, view_dir: vec3<f32>) -> vec3<f32> {
    let q0 = dpdx(position);
    let q1 = dfdy3(position);
    let st0 = dpdx(uv);
    let st1 = dfdy2(uv);
    let q1perp = cross(q1, surface);
    let q0perp = cross(surface, q0);
    let tangent = normalize(q1perp * st0.x + q0perp * st1.x);
    let bitangent = normalize(q1perp * st0.y + q0perp * st1.y);
    return normalize(vec3<f32>(dot(view_dir, tangent), dot(view_dir, bitangent), dot(view_dir, surface)));
}

// Khronos PBR Neutral tone mapping, with the exposure, as three's NeutralToneMapping.
fn neutral_tone(c_in: vec3<f32>, exposure: f32) -> vec3<f32> {
    var color = c_in * exposure;
    let start = 0.8 - 0.04;
    let desaturation = 0.15;
    let x = min(color.r, min(color.g, color.b));
    var offset = 0.04;
    if (x < 0.08) {
        offset = x - 6.25 * x * x;
    }
    color = color - offset;
    let peak = max(color.r, max(color.g, color.b));
    if (peak < start) {
        return color;
    }
    let d = 1.0 - start;
    let new_peak = 1.0 - d * d / (peak + d - start);
    color = color * (new_peak / peak);
    let g = 1.0 - 1.0 / (desaturation * (peak - new_peak) + 1.0);
    return mix(color, vec3<f32>(new_peak), g);
}

fn pack_normal(n: vec3<f32>) -> vec3<f32> {
    return n * 0.5 + 0.5;
}
