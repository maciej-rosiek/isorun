fn water_colour(in: VertexOut, s: Surface, alpha: f32) -> vec4<f32> {
    return vec4<f32>(light_surface(MODEL_PHONG, s), alpha);
}
