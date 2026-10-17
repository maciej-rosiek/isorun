// Tint (the browsers' WebGPU) checks that derivatives and implicit-lod samples run in uniform
// control flow more strictly than naga; the shaders take them after a discard or in a branch on
// purpose, where the neighbours' values are close enough.
diagnostic(off, derivative_uniformity);

// The shadow maps: the casters seen from the sun (an orthographic depth) or from a torch (the
// distance to it over its reach, one of six views), alpha tested like the scene's sprites.

struct ShadowView {
    view_proj: mat4x4<f32>,
    // the light's world position, w its reach
    light: vec4<f32>,
    // x: 1 for a torch
    mode: vec4<f32>,
};

struct Draw {
    model: mat4x4<f32>,
    tint: vec4<f32>,
    look: vec4<f32>,
    harm: vec4<f32>,
    aura: vec4<f32>,
};

@group(0) @binding(0) var<uniform> shadow_view: ShadowView;
@group(1) @binding(0) var colour_map: texture_2d<f32>;
@group(1) @binding(1) var normal_map: texture_2d<f32>;
@group(1) @binding(2) var surface_map: texture_2d<f32>;
@group(1) @binding(3) var page_sampler: sampler;
@group(2) @binding(0) var<uniform> draw: Draw;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) uv_rect: vec4<f32>,
    @location(3) colour: vec3<f32>,
    @location(4) normal: vec3<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world: vec3<f32>,
};

fn place(v: VertexIn, local: vec3<f32>) -> VertexOut {
    var out: VertexOut;
    let world = draw.model * vec4<f32>(local, 1.0);
    out.clip = shadow_view.view_proj * world;
    out.uv = v.uv;
    out.world = world.xyz;
    return out;
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    return place(v, v.position);
}

@vertex
fn vs_actor(v: VertexIn) -> VertexOut {
    return place(v, v.position);
}

@fragment
fn fs_sun(in: VertexOut) {
    if (textureSample(colour_map, page_sampler, in.uv).a < 0.5) {
        discard;
    }
}

@fragment
fn fs_point(in: VertexOut) -> @builtin(frag_depth) f32 {
    if (textureSample(colour_map, page_sampler, in.uv).a < 0.5) {
        discard;
    }
    return length(in.world - shadow_view.light.xyz) / shadow_view.light.w;
}
