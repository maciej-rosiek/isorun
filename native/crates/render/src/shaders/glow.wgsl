// Soft round sprites that face the camera, added to the light: the torches' flames and the dust
// in their light (torches.ts, dust.ts).

struct Glow {
    // the material's rgb colour (linear), w size in cells
    colour: vec4<f32>,
    // the texture's colour at the middle, in sRGB 0..1
    inner: vec4<f32>,
    // x: 1 for dust (the motes move and fade in the shader), y: dust strength
    mode: vec4<f32>,
};

@group(1) @binding(0) var<uniform> glow: Glow;

struct VertexIn {
    @location(0) corner: vec2<f32>,
    @location(1) seed: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fade: f32,
};

@vertex
fn vs_glow(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    var centre = v.seed.xyz;
    var fade = 1.0;
    if (glow.mode.x > 0.5) {
        let phase = v.seed.w;
        let t = time();
        let age = fract(t * 0.12 + phase * 0.137);
        centre = centre + vec3<f32>(sin(t * 0.7 + phase) * 0.08, age * 0.8, cos(t * 0.5 + phase * 1.7) * 0.08);
        fade = sin(age * 3.14159265) * glow.mode.y;
    }
    let right = (frame.inv_view * vec4<f32>(1.0, 0.0, 0.0, 0.0)).xyz;
    let up = (frame.inv_view * vec4<f32>(0.0, 1.0, 0.0, 0.0)).xyz;
    let world = centre + (right * v.corner.x + up * v.corner.y) * glow.colour.w;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.uv = v.corner + 0.5;
    out.fade = fade;
    return out;
}

struct FragmentOut {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) emission: vec4<f32>,
};

// A canvas radial gradient from the colour at the middle to nothing at the edge (materials.ts `radialTexture`).
@fragment
fn fs_glow(in: VertexOut) -> FragmentOut {
    let r = length(in.uv - 0.5) * 2.0;
    let t = saturate(1.0 - (r - 0.0625) / 0.9375);
    // the gradient runs from the colour to transparent black in sRGB bytes, unpremultiplied: the
    // colour darkens toward the edge as the alpha falls, and the texture is decoded to linear light
    let srgb = glow.inner.rgb * t;
    let decoded = select(pow((srgb + 0.055) / 1.055, vec3<f32>(2.4)), srgb / 12.92, srgb <= vec3<f32>(0.04045));
    var out: FragmentOut;
    out.colour = vec4<f32>(decoded * glow.colour.rgb, t * in.fade);
    out.normal = vec4<f32>(0.0);
    out.emission = vec4<f32>(0.0);
    return out;
}
