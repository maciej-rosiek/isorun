// The drifting fog over the unexplored cells (fog.ts): stacked layers with a dense body and a
// puffy edge that the noise moves; it never covers an explored cell on the screen.

struct Fog {
    colour: vec4<f32>,
    // density, softness, layer height, blend
    params: vec4<f32>,
    // origin x, y, size w, h
    grid: vec4<f32>,
};

@group(1) @binding(0) var<uniform> fog: Fog;
@group(1) @binding(1) var field_from: texture_2d<f32>;
@group(1) @binding(2) var field_to: texture_2d<f32>;
@group(1) @binding(3) var mask_from: texture_2d<f32>;
@group(1) @binding(4) var mask_to: texture_2d<f32>;

const LAYERS: f32 = 10.0;
const NOISE_SCALE: f32 = 0.12;
const DRIFT: f32 = 0.015;
const EDGE: f32 = 0.35;
const EDGE_NOISE: f32 = 0.8;
const DOME: f32 = 0.45;
const LAYER_OPACITY: f32 = 0.3;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) layer: f32,
};

@vertex
fn vs_fog(@location(0) position: vec3<f32>, @builtin(instance_index) instance: u32) -> VertexOut {
    var out: VertexOut;
    let layer = f32(instance) / (LAYERS - 1.0);
    let world = position + vec3<f32>(0.0, layer * fog.params.z, 0.0);
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.layer = layer;
    return out;
}

struct FragmentOut {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) emission: vec4<f32>,
};

@fragment
fn fs_fog(in: VertexOut) -> FragmentOut {
    let t = time();
    let place = in.world.xz + in.layer * 3.7;
    let slow = textureSampleLevel(noise_map, repeat_sampler, place * NOISE_SCALE + vec2<f32>(t * DRIFT, t * DRIFT * 0.6), 0.0).r;
    let fast = textureSampleLevel(noise_map, repeat_sampler, place * (NOISE_SCALE * 2.3) - vec2<f32>(t * DRIFT * 1.7, t * DRIFT * 0.4), 0.0).r;
    let puffs = smoothstep(0.25, 0.75, slow * 0.6 + fast * 0.4);
    let blend = smoothstep(0.0, 1.0, fog.params.w);
    let here = (in.world.xz - fog.grid.xy) / fog.grid.zw;
    let field = mix(textureSampleLevel(field_from, linear_sampler, here, 0.0).r, textureSampleLevel(field_to, linear_sampler, here, 0.0).r, blend);
    let forward = -(frame.inv_view * vec4<f32>(0.0, 0.0, 1.0, 0.0)).xyz;
    let ray = mix(forward, in.world - frame.camera.xyz, frame.camera.w);
    let ground = (in.world.xz - ray.xz * (in.world.y / min(ray.y, -0.01)) - fog.grid.xy) / fog.grid.zw;
    let explored = smoothstep(0.2, 0.6, mix(textureSampleLevel(mask_from, linear_sampler, ground, 0.0).r, textureSampleLevel(mask_to, linear_sampler, ground, 0.0).r, blend));
    let body = field + (puffs - 0.5) * EDGE_NOISE - in.layer * DOME;
    let width = fog.params.y * EDGE + 0.02;
    let alpha = smoothstep(EDGE - width, EDGE + width, body) * smoothstep(0.0, 0.1, field) * (1.0 - explored) * fog.params.x * LAYER_OPACITY;
    var out: FragmentOut;
    out.colour = vec4<f32>(fog.colour.rgb, alpha);
    out.normal = vec4<f32>(0.0);
    out.emission = vec4<f32>(0.0);
    return out;
}
