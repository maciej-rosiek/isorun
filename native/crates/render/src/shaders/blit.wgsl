// A full-screen triangle that copies a texture: mipmaps, and the view onto the window.

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> VertexOut {
    var out: VertexOut;
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    out.clip = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@fragment
fn fs_copy(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(source, source_sampler, in.uv, 0.0);
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// Onto a window whose format is not sRGB: the encoding is done here.
@fragment
fn fs_copy_encode(in: VertexOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(source, source_sampler, in.uv, 0.0);
    return vec4<f32>(to_srgb(c.rgb), c.a);
}
