// Tint (the browsers' WebGPU) checks that derivatives and implicit-lod samples run in uniform
// control flow more strictly than naga; the shaders take them after a discard or in a branch on
// purpose, where the neighbours' values are close enough.
diagnostic(off, derivative_uniformity);

// Shared by the world's shaders: the frame's uniforms, the lights and shadows, the memory mask,
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
    // x = first shadow layer (-1 for none), y = bias, z = normal bias, w = far
    shadow: vec4<f32>,
    // world-space position, w unused
    world: vec4<f32>,
};

struct Frame {
    view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    inv_view: mat4x4<f32>,
    sun_matrix: mat4x4<f32>,
    // world-space camera position, w = 1 for perspective
    camera: vec4<f32>,
    sky: vec4<f32>,
    ground: vec4<f32>,
    // view-space direction to the sun, w = 1 when it casts a shadow
    sun_dir: vec4<f32>,
    sun_colour: vec4<f32>,
    // bias, normal bias, texel size, radius
    sun_shadow: vec4<f32>,
    // wrap, bands, relief, depth
    shading0: vec4<f32>,
    // sprites, gloss, rim, cavity
    shading1: vec4<f32>,
    // variation, wetness, glow, sparkle
    shading2: vec4<f32>,
    // depth steps, liquid depth, statuses shown, time
    shading3: vec4<f32>,
    // width, height, light count, point shadow texel radius
    viewport: vec4<f32>,
    // view-space up, w = dust strength
    up: vec4<f32>,
    lights: array<Light, MAX_LIGHTS>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var sun_shadow_map: texture_depth_2d;
@group(0) @binding(2) var point_shadow_map: texture_depth_2d_array;
@group(0) @binding(3) var shadow_sampler: sampler_comparison;
@group(0) @binding(4) var memory_map: texture_2d<f32>;
@group(0) @binding(5) var wet_map: texture_2d<f32>;
@group(0) @binding(6) var nearest_sampler: sampler;
@group(0) @binding(7) var linear_sampler: sampler;
@group(0) @binding(8) var noise_map: texture_2d<f32>;
@group(0) @binding(9) var repeat_sampler: sampler;

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
    return textureSampleLevel(memory_map, nearest_sampler, (cell + 0.5) / MEMORY_CELLS, 0.0).r;
}

// How wet the ground is here (r) and how deep under see-through water (g), smooth between cells.
fn wetness_at(world: vec3<f32>) -> vec2<f32> {
    return textureSampleLevel(wet_map, linear_sampler, world.xz / MEMORY_CELLS, 0.0).rg;
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

// three's vogelDiskSample: the i-th of n points spread evenly over the unit disc, turned by phi.
fn vogel_disk(i: f32, n: f32, phi: f32) -> vec2<f32> {
    let r = sqrt((i + 0.5) / n);
    let theta = i * 2.399963229728653 + phi;
    return vec2<f32>(cos(theta), sin(theta)) * r;
}

// The turn of the shadows' sampling pattern at a pixel (three's interleavedGradientNoise times 2 pi),
// so the five samples band nowhere.
fn shadow_turn(screen: vec2<f32>) -> f32 {
    return interleaved_gradient_noise(screen) * 6.28318530718;
}

// three's PCFShadowFilter: five compared samples on a Vogel disc turned for each pixel, each
// filtered by the comparison sampler (four texels), over the shadow's radius.
fn sun_shadow(world: vec3<f32>, normal_world: vec3<f32>, screen: vec2<f32>) -> f32 {
    if (frame.sun_dir.w < 0.5) {
        return 1.0;
    }
    let p = frame.sun_matrix * vec4<f32>(world + normal_world * frame.sun_shadow.y, 1.0);
    let ndc = p.xyz / p.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z > 1.0) {
        return 1.0;
    }
    let depth = ndc.z + frame.sun_shadow.x;
    let r = frame.sun_shadow.z * frame.sun_shadow.w;
    let phi = shadow_turn(screen);
    var sum = 0.0;
    for (var i = 0; i < 5; i = i + 1) {
        sum = sum + textureSampleCompareLevel(sun_shadow_map, shadow_sampler, uv + vogel_disk(f32(i), 5.0, phi) * r, depth);
    }
    return sum / 5.0;
}

// The face of a cube a direction points through, and where on it: the same layout the shadow
// pass renders its six views in (`cube_face_view` in shadows.rs).
fn cube_face(d: vec3<f32>) -> vec3<f32> {
    let a = abs(d);
    if (a.x >= a.y && a.x >= a.z) {
        if (d.x > 0.0) { return vec3<f32>(0.0, -d.z / a.x, -d.y / a.x); }
        return vec3<f32>(1.0, d.z / a.x, -d.y / a.x);
    }
    if (a.y >= a.z) {
        if (d.y > 0.0) { return vec3<f32>(2.0, d.x / a.y, d.z / a.y); }
        return vec3<f32>(3.0, d.x / a.y, -d.z / a.y);
    }
    if (d.z > 0.0) { return vec3<f32>(4.0, d.x / a.z, -d.y / a.z); }
    return vec3<f32>(5.0, -d.x / a.z, -d.y / a.z);
}

// three's PointShadowFilter: five compared samples around the direction to the fragment, on a
// Vogel disc in the plane across it, turned for each pixel.
fn point_shadow(light: Light, world: vec3<f32>, normal_world: vec3<f32>, screen: vec2<f32>) -> f32 {
    if (light.shadow.x < 0.0) {
        return 1.0;
    }
    let p = world + normal_world * light.shadow.z - light.world.xyz;
    let depth = length(p) / light.shadow.w + light.shadow.y;
    let dir = normalize(p);
    let a = abs(dir);
    let tangent = normalize(cross(dir, select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), a.x > a.z)));
    let bitangent = cross(dir, tangent);
    let texel = frame.viewport.w;
    let phi = shadow_turn(screen);
    var sum = 0.0;
    for (var i = 0; i < 5; i = i + 1) {
        let o = vogel_disk(f32(i), 5.0, phi);
        let face = cube_face(dir + (tangent * o.x + bitangent * o.y) * texel);
        let uv = vec2<f32>(face.y * 0.5 + 0.5, face.z * 0.5 + 0.5);
        sum = sum + textureSampleCompareLevel(point_shadow_map, shadow_sampler, uv, i32(light.shadow.x) + i32(face.x), depth);
    }
    return sum / 5.0;
}

struct Surface {
    // view space
    position: vec3<f32>,
    normal: vec3<f32>,
    view_dir: vec3<f32>,
    // world space, for the shadows
    world: vec3<f32>,
    normal_world: vec3<f32>,
    diffuse: vec3<f32>,
    specular: vec3<f32>,
    shininess: f32,
    rim: f32,
    receive_shadow: bool,
    // the fragment's pixel, which turns the shadows' sampling pattern
    screen: vec2<f32>,
};

// ClayLightingModel.direct: wrapped, banded diffuse, a Blinn-Phong highlight, a rim light.
fn clay_direct(s: Surface, dir: vec3<f32>, colour: vec3<f32>) -> vec3<f32> {
    let wrap = frame.shading0.x;
    let bands = frame.shading0.y;
    let wrapped = saturate1((dot(s.normal, dir) + wrap) / (wrap + 1.0));
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

fn direct(model: u32, s: Surface, dir: vec3<f32>, colour: vec3<f32>) -> vec3<f32> {
    if (model == MODEL_PHONG) {
        return phong_direct(s, dir, colour);
    }
    if (model == MODEL_LAMBERT) {
        return lambert_direct(s, dir, colour);
    }
    return clay_direct(s, dir, colour);
}

// Every light on the surface: the hemisphere light, the sun and the point lights.
fn light_surface(model: u32, s: Surface) -> vec3<f32> {
    let weight = dot(s.normal, frame.up.xyz) * 0.5 + 0.5;
    let irradiance = mix(frame.ground.rgb, frame.sky.rgb, weight);
    var out = irradiance * s.diffuse / PI;
    var sun = frame.sun_colour.rgb;
    if (s.receive_shadow) {
        sun = sun * sun_shadow(s.world, s.normal_world, s.screen);
    }
    out = out + direct(model, s, frame.sun_dir.xyz, sun);
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
        var colour = light.colour.rgb * distance_attenuation(d, light.position.w, light.colour.w);
        if (s.receive_shadow) {
            colour = colour * point_shadow(light, s.world, s.normal_world, s.screen);
        }
        out = out + direct(model, s, to_light / d, colour);
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
