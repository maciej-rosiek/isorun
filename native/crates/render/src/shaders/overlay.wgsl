// The pointer on the ground (render-iso-3d.ts `aimMaterial`): the steering arrow, the route's dots
// and the target and cursor squares, in a flat colour that the screen effects leave as it is
// (the normal's alpha 0 marks it).

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) colour: vec3<f32>,
};

@vertex
fn vs_overlay(@location(0) position: vec2<f32>, @location(1) m0: vec4<f32>, @location(2) m1: vec4<f32>, @location(3) m2: vec4<f32>, @location(4) m3: vec4<f32>, @location(5) colour: vec4<f32>) -> VertexOut {
    var out: VertexOut;
    let model = mat4x4<f32>(m0, m1, m2, m3);
    out.clip = frame.view_proj * model * vec4<f32>(position.x, 0.0, position.y, 1.0);
    out.colour = colour.rgb;
    return out;
}

struct FragmentOut {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) emission: vec4<f32>,
};

@fragment
fn fs_overlay(in: VertexOut) -> FragmentOut {
    var out: FragmentOut;
    out.colour = vec4<f32>(in.colour, 1.0);
    out.normal = vec4<f32>(0.5, 1.0, 0.5, 0.0);
    out.emission = vec4<f32>(0.0);
    return out;
}
