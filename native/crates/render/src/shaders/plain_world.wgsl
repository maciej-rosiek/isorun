fn material_at(in: VertexOut, at: vec2<f32>) -> vec2<f32> {
    return at;
}

fn material_out(in: VertexOut, at: vec2<f32>, s: Surface, light: vec3<f32>, alpha: f32, face: f32, kind: u32) -> FragmentOut {
    return outputs(lit_surface(MODEL_CLAY, s, light), alpha, in, face, light);
}
