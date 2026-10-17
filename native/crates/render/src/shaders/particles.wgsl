// Instanced quads for what a fight and a step leave (combat.ts, steps.ts): drops of blood and
// sparks that fly, blood that stays on the floor, dust puffs and footprints.

struct Particles {
    colour: vec4<f32>,
};

@group(1) @binding(0) var<uniform> particles: Particles;

struct VertexIn {
    @location(0) corner: vec2<f32>,
    @location(1) m0: vec4<f32>,
    @location(2) m1: vec4<f32>,
    @location(3) m2: vec4<f32>,
    @location(4) m3: vec4<f32>,
    // x: fade
    @location(5) extra: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) view_pos: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) fade: f32,
};

@vertex
fn vs_particle(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    let world = model * vec4<f32>(v.corner, 0.0, 1.0);
    out.world = world.xyz;
    out.view_pos = (frame.view * world).xyz;
    out.clip = frame.view_proj * world;
    out.normal = normalize((frame.view * (model * vec4<f32>(0.0, 0.0, 1.0, 0.0))).xyz);
    out.uv = vec2<f32>(v.corner.x + 0.5, 0.5 - v.corner.y);
    out.fade = v.extra.x;
    return out;
}

struct FragmentOut {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) emission: vec4<f32>,
};

fn result(colour: vec3<f32>, alpha: f32, normal: vec3<f32>) -> FragmentOut {
    var out: FragmentOut;
    out.colour = vec4<f32>(colour, alpha);
    out.normal = vec4<f32>(pack_normal(normal), 1.0);
    out.emission = vec4<f32>(0.0);
    return out;
}

fn lambert(in: VertexOut, diffuse: vec3<f32>, face: f32) -> vec3<f32> {
    var s: Surface;
    s.position = in.view_pos;
    s.screen = in.clip.xy;
    s.normal = in.normal * face;
    s.view_dir = normalize(-in.view_pos);
    s.world = in.world;
    s.normal_world = vec3<f32>(0.0, 1.0, 0.0);
    s.diffuse = diffuse;
    s.specular = vec3<f32>(0.0);
    s.shininess = 1.0;
    s.rim = 0.0;
    s.receive_shadow = false;
    return light_surface(MODEL_LAMBERT, s);
}

@fragment
fn fs_flat(in: VertexOut) -> FragmentOut {
    return result(particles.colour.rgb, 1.0, in.normal);
}

@fragment
fn fs_splat(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {
    let edge = length(in.uv - 0.5) * 2.0 + (textureSampleLevel(noise_map, repeat_sampler, in.world.xz * 1.3, 0.0).r - 0.5) * 0.9;
    let alpha = smoothstep_down(0.8, 0.65, edge) * 0.92;
    return result(lambert(in, particles.colour.rgb, select(-1.0, 1.0, front)), alpha, in.normal);
}

@fragment
fn fs_puff(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {
    let puffy = textureSampleLevel(noise_map, repeat_sampler, in.uv * 0.4 + in.world.xz * 0.7, 0.0).r;
    let round = max(1.0 - length(in.uv - 0.5) * 2.0, 0.0);
    let alpha = smoothstep(0.1, 0.9, round * puffy * 2.0) * in.fade * 0.45;
    return result(lambert(in, particles.colour.rgb, select(-1.0, 1.0, front)), alpha, in.normal);
}

@fragment
fn fs_print(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {
    let oval = length((in.uv - 0.5) * 2.0);
    let alpha = smoothstep_down(1.0, 0.6, oval) * in.fade * 0.75;
    return result(lambert(in, particles.colour.rgb, select(-1.0, 1.0, front)), alpha, in.normal);
}
