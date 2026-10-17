// The screen effects (post.ts): ambient occlusion, blurs for the tilt-shift and the bloom, and the
// composite with the painted filter, outlines, pixels, the colour grade, the vignette, the danger
// edge and the tone mapping. Each entry point is one pass over a full-screen triangle.

struct Post {
    // ao intensity, ao radius (cells), ao samples, depth range
    ao: vec4<f32>,
    // outline thickness, strength, painted radius, pixel size times scale
    edges: vec4<f32>,
    // outline colour, w exposure
    outline_colour: vec4<f32>,
    // bloom strength, radius, threshold, emission strength
    bloom: vec4<f32>,
    // blur, focus, falloff, danger
    tilt: vec4<f32>,
    // saturation, vignette, pixel origin x, y
    grade: vec4<f32>,
    shadows: vec4<f32>,
    highlights: vec4<f32>,
    // flags: ao, outline, painted, pixelate
    flags0: vec4<f32>,
    // flags: bloom, tilt-shift, emission, perspective
    flags1: vec4<f32>,
    // full size w, h, 1/w, 1/h
    size: vec4<f32>,
    proj: mat4x4<f32>,
    inv_proj: mat4x4<f32>,
};

struct Blur {
    // texel step x, y (in uv), sigma, taps each side
    step: vec4<f32>,
    // threshold, smooth width, use high pass, unused
    params: vec4<f32>,
};

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

@group(0) @binding(0) var<uniform> post: Post;
@group(0) @binding(1) var linear_s: sampler;
@group(0) @binding(2) var point_s: sampler;
// nearest, for the depth: sampled, not loaded, so that WebGL2 has a sampler to make it complete
@group(0) @binding(3) var depth_s: sampler;

// ---------------------------------------------------------------- ambient occlusion

// the depth as an unfilterable float texture, not texture_depth_2d: WebGL2 (GLSL) has no
// textureLoad from a depth texture
@group(1) @binding(0) var depth_tex: texture_2d<f32>;
@group(1) @binding(1) var normal_tex: texture_2d<f32>;

fn view_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let p = post.inv_proj * ndc;
    return p.xyz / p.w;
}

fn depth_at(uv: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(depth_tex));
    let c = clamp(vec2<i32>(uv * size), vec2<i32>(0), vec2<i32>(size) - 1);
    return textureSampleLevel(depth_tex, depth_s, (vec2<f32>(c) + 0.5) / size, 0.0).r;
}

fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

@group(1) @binding(2) var ao_noise: texture_2d<f32>;

// three's getScreenPosition: a view-space point to its place on the screen, y down.
fn screen_position(p: vec3<f32>) -> vec2<f32> {
    let clip = post.proj * vec4<f32>(p, 1.0);
    let uv = clip.xy / clip.w * 0.5 + 0.5;
    return vec2<f32>(uv.x, 1.0 - uv.y);
}

// Ground-truth ambient occlusion, as three's GTAONode computes it (thickness 1, distance exponent
// 1, distance fall-off 1, scale 1, no temporal rotation): a few slices through the view around
// each pixel, the horizon found on both sides of each, the cosine-weighted visible arc between.
@fragment
fn fs_ao(in: VertexOut) -> @location(0) vec4<f32> {
    let depth = depth_at(in.uv);
    if (depth >= 1.0) {
        discard;
    }
    let vp = view_position(in.uv, depth);
    let view_normal = normalize(textureSampleLevel(normal_tex, point_s, in.uv, 0.0).xyz * 2.0 - 1.0);
    let radius = post.ao.y;
    let noise_size = vec2<f32>(textureDimensions(ao_noise));
    let target_size = vec2<f32>(textureDimensions(depth_tex)) * post.bloom.y;
    let noise_uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * (target_size / noise_size);
    let noise_texel = textureLoad(ao_noise, vec2<i32>(fract(noise_uv) * noise_size) % vec2<i32>(noise_size), 0);
    let random_vec = noise_texel.xyz * 2.0 - 1.0;
    let tangent = normalize(vec3<f32>(random_vec.xy, 0.0));
    let bitangent = vec3<f32>(-tangent.y, tangent.x, 0.0);
    let kernel = mat3x3<f32>(tangent, bitangent, vec3<f32>(0.0, 0.0, 1.0));
    let samples = i32(post.ao.z);
    var directions = 3;
    if (samples >= 30) {
        directions = 5;
    }
    let steps = (samples + directions - 1) / directions;
    var ao = 0.0;
    for (var i = 0; i < directions; i = i + 1) {
        let angle = f32(i) / f32(directions) * 3.141592653589793;
        let reach = 0.5 + 0.5 * noise_texel.w;
        let sample_dir = normalize(kernel * vec3<f32>(cos(angle), sin(angle), 0.0));
        let view_dir = normalize(-vp);
        let slice_bitangent = normalize(cross(sample_dir, view_dir));
        let slice_tangent = cross(slice_bitangent, view_dir);
        let proj_raw = view_normal - slice_bitangent * dot(view_normal, slice_bitangent);
        let proj_len = length(proj_raw);
        let proj_n = proj_raw / max(proj_len, 0.0001);
        let n_sin = dot(proj_n, slice_tangent);
        let n_cos = clamp(dot(proj_n, view_dir), 0.0, 1.0);
        let angle_n = select(-1.0, 1.0, n_sin >= 0.0) * acos(n_cos);
        let tangent_to_normal = cross(proj_n, slice_bitangent);
        var cos_horizons = vec2<f32>(dot(view_dir, tangent_to_normal), dot(view_dir, -tangent_to_normal));
        for (var j = 0; j < steps; j = j + 1) {
            let offset = sample_dir * radius * reach * (f32(j + 1) / f32(steps));
            let fall = mix(1.0, 2.0 / f32(j + 2), 1.0);
            let sx = screen_position(vp + offset);
            let dx = view_position(sx, depth_at(sx)) - vp;
            if (abs(dx.z) < 1.0) {
                cos_horizons.x = cos_horizons.x + max(0.0, (dot(view_dir, normalize(dx)) - cos_horizons.x) * fall);
            }
            let sy = screen_position(vp - offset);
            let dy = view_position(sy, depth_at(sy)) - vp;
            if (abs(dy.z) < 1.0) {
                cos_horizons.y = cos_horizons.y + max(0.0, (dot(view_dir, normalize(dy)) - cos_horizons.y) * fall);
            }
        }
        let h_pos = acos(clamp(cos_horizons.y, -1.0, 1.0));
        let h_neg = -acos(clamp(cos_horizons.x, -1.0, 1.0));
        let term_pos = -cos(h_pos * 2.0 - angle_n) + n_cos + h_pos * 2.0 * n_sin;
        let term_neg = -cos(h_neg * 2.0 - angle_n) + n_cos + h_neg * 2.0 * n_sin;
        ao = ao + proj_len * (term_pos + term_neg) * 0.25;
    }
    let result = clamp(ao / f32(directions), 0.0, 1.0);
    return vec4<f32>(result, result, result, 1.0);
}

// ---------------------------------------------------------------- blurs

@group(1) @binding(0) var source_tex: texture_2d<f32>;
@group(2) @binding(0) var<uniform> blur: Blur;

fn luminance709(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// A Gaussian along one direction, `taps` texels each side.
@fragment
fn fs_blur(in: VertexOut) -> @location(0) vec4<f32> {
    let sigma = blur.step.z;
    let taps = i32(blur.step.w);
    var sum = textureSampleLevel(source_tex, linear_s, in.uv, 0.0);
    var weight = 1.0;
    for (var i = 1; i <= taps; i = i + 1) {
        let x = f32(i);
        let w = exp(-0.5 * x * x / (sigma * sigma));
        let o = blur.step.xy * x;
        sum = sum + (textureSampleLevel(source_tex, linear_s, in.uv + o, 0.0) + textureSampleLevel(source_tex, linear_s, in.uv - o, 0.0)) * w;
        weight = weight + 2.0 * w;
    }
    return sum / weight;
}

// The bloom's high pass: what is brighter than the threshold, softly.
@fragment
fn fs_high_pass(in: VertexOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(source_tex, linear_s, in.uv, 0.0);
    let l = luminance709(c.rgb);
    let alpha = smoothstep(blur.params.x, blur.params.x + blur.params.y, l);
    return vec4<f32>(c.rgb * alpha, 1.0);
}

@fragment
fn fs_down(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(source_tex, linear_s, in.uv, 0.0);
}

// ---------------------------------------------------------------- composite

@group(1) @binding(0) var scene_colour: texture_2d<f32>;
@group(1) @binding(1) var scene_normal: texture_2d<f32>;
@group(1) @binding(2) var scene_depth: texture_2d<f32>;
@group(1) @binding(3) var ao_tex: texture_2d<f32>;
@group(1) @binding(4) var blur_tex: texture_2d<f32>;
@group(1) @binding(5) var bloom_tex: texture_2d<f32>;
@group(1) @binding(6) var emission_tex: texture_2d<f32>;

fn occlusion(uv: vec2<f32>) -> f32 {
    if (post.flags0.x < 0.5) {
        return 1.0;
    }
    return mix(1.0, textureSampleLevel(ao_tex, linear_s, uv, 0.0).r, post.ao.x);
}

fn shaded(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(scene_colour, linear_s, uv, 0.0).rgb * occlusion(uv);
}

fn depth_load(uv: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(scene_depth));
    let c = clamp(vec2<i32>(uv * size), vec2<i32>(0), vec2<i32>(size) - 1);
    return textureSampleLevel(scene_depth, depth_s, (vec2<f32>(c) + 0.5) / size, 0.0).r;
}

fn linear_depth(uv: vec2<f32>) -> f32 {
    return -view_position(uv, depth_load(uv)).z;
}

// Kuwahara: the mean colour of the most even of the four squares that meet at the pixel.
fn painted(uv: vec2<f32>) -> vec3<f32> {
    let step = vec2<f32>(post.edges.z) * post.size.zw;
    var best = vec3<f32>(0.0);
    var best_variance = 1e9;
    for (var q = 0; q < 4; q = q + 1) {
        let sx = select(-1.0, 1.0, (q & 1) == 1);
        let sy = select(-1.0, 1.0, (q & 2) == 2);
        var sum = vec3<f32>(0.0);
        var light = 0.0;
        var light2 = 0.0;
        for (var i = 0; i <= 3; i = i + 1) {
            for (var j = 0; j <= 3; j = j + 1) {
                let c = shaded(uv + vec2<f32>(f32(i) * sx, f32(j) * sy) * step);
                let l = luminance709(c);
                sum = sum + c;
                light = light + l;
                light2 = light2 + l * l;
            }
        }
        let mean = light / 16.0;
        let variance = light2 / 16.0 - mean * mean;
        if (variance < best_variance) {
            best_variance = variance;
            best = sum / 16.0;
        }
    }
    return best;
}

const OUTLINE_DEPTH: f32 = 0.25;
const OUTLINE_NORMAL: f32 = 0.35;
const DANGER_COLOUR: vec3<f32> = vec3<f32>(0.45, 0.03, 0.03);

fn edge_at(uv: vec2<f32>) -> f32 {
    let centre_depth = linear_depth(uv);
    let centre_normal = textureSampleLevel(scene_normal, point_s, uv, 0.0).xyz * 2.0 - 1.0;
    var edge = 0.0;
    let offsets = array<vec2<f32>, 4>(vec2<f32>(1.0, 0.0), vec2<f32>(-1.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(0.0, -1.0));
    for (var i = 0; i < 4; i = i + 1) {
        let o = offsets[i] * post.edges.x * post.size.zw;
        let farther = smoothstep(OUTLINE_DEPTH, OUTLINE_DEPTH * 2.0, linear_depth(uv + o) - centre_depth);
        let n = textureSampleLevel(scene_normal, point_s, uv + o, 0.0).xyz * 2.0 - 1.0;
        let turned = smoothstep(OUTLINE_NORMAL, OUTLINE_NORMAL * 2.0, 1.0 - dot(centre_normal, n));
        edge = max(edge, max(farther, turned));
    }
    return edge;
}

override ENCODE_SRGB: bool = false;

fn srgb_from_linear(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}

@fragment
fn fs_composite(in: VertexOut) -> @location(0) vec4<f32> {
    var uv = in.uv;
    if (post.flags0.w > 0.5) {
        let size = max(post.edges.w, 1.0);
        let px = in.uv * post.size.xy;
        let square = floor((px - post.grade.zw) / size) + 0.5;
        uv = (square * size + post.grade.zw) * post.size.zw;
    }
    let colour = textureSampleLevel(scene_colour, linear_s, in.uv, 0.0);
    var image = shaded(uv);
    if (post.flags0.z > 0.5) {
        image = painted(uv);
    }
    if (post.flags0.y > 0.5) {
        image = mix(image, post.outline_colour.rgb, edge_at(uv) * post.edges.y);
    }
    if (post.flags1.x > 0.5) {
        image = image + textureSampleLevel(bloom_tex, linear_s, uv, 0.0).rgb;
    }
    if (post.flags1.y > 0.5) {
        let blurred = textureSampleLevel(blur_tex, linear_s, uv, 0.0).rgb * occlusion(uv);
        let away = smoothstep(post.tilt.y, post.tilt.y + post.tilt.z, abs(in.uv.y - 0.5) * 2.0);
        image = mix(image, blurred, away);
    }
    if (post.flags1.z > 0.5) {
        image = image + textureSampleLevel(emission_tex, linear_s, uv, 0.0).rgb * post.bloom.w;
    }
    let graded = mix(vec3<f32>(luminance709(image)), image, post.grade.x);
    let tint = mix(post.shadows.rgb / luminance709(post.shadows.rgb), post.highlights.rgb / luminance709(post.highlights.rgb), smoothstep(0.0, 0.6, luminance709(graded)));
    let r = length(in.uv - 0.5) * 1.414;
    let corners = 1.0 - pow(r, 2.0) * post.grade.y;
    let danger = smoothstep(0.6, 1.3, r) * post.tilt.w * 0.3;
    let shown = mix(graded * tint * corners, DANGER_COLOUR, danger);
    let overlay = 1.0 - textureSampleLevel(scene_normal, point_s, in.uv, 0.0).a;
    var out = mix(shown, colour.rgb, overlay);
    // a look at one of the effects' images alone, for checking them
    let debug = i32(post.ao.w);
    if (debug == 1) {
        out = textureSampleLevel(emission_tex, linear_s, uv, 0.0).rgb;
    } else if (debug == 2) {
        out = vec3<f32>(occlusion(uv)) / post.outline_colour.w;
    } else if (debug == 3) {
        out = textureSampleLevel(scene_normal, point_s, uv, 0.0).rgb / post.outline_colour.w;
    } else if (debug == 4) {
        out = textureSampleLevel(blur_tex, linear_s, uv, 0.0).rgb;
    }
    let toned = neutral_tone(out, post.outline_colour.w);
    // an output that is not sRGB (WebGL2 has no sRGB view of a plain texture) gets the encoding here
    if (ENCODE_SRGB) {
        return vec4<f32>(srgb_from_linear(toned), 1.0);
    }
    return vec4<f32>(toned, 1.0);
}

// Khronos PBR Neutral, as three's NeutralToneMapping, after the exposure.
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

// ---------------------------------------------------------------- the bloom's mips together

struct BloomSum {
    // the five mips' factors times the strength
    factors0: vec4<f32>,
    factors1: vec4<f32>,
};

@group(1) @binding(0) var mip0: texture_2d<f32>;
@group(1) @binding(1) var mip1: texture_2d<f32>;
@group(1) @binding(2) var mip2: texture_2d<f32>;
@group(1) @binding(3) var mip3: texture_2d<f32>;
@group(1) @binding(4) var mip4: texture_2d<f32>;
@group(2) @binding(0) var<uniform> bloom_sum: BloomSum;

@fragment
fn fs_bloom_sum(in: VertexOut) -> @location(0) vec4<f32> {
    let f0 = bloom_sum.factors0;
    let f1 = bloom_sum.factors1;
    let c = textureSampleLevel(mip0, linear_s, in.uv, 0.0).rgb * f0.x
        + textureSampleLevel(mip1, linear_s, in.uv, 0.0).rgb * f0.y
        + textureSampleLevel(mip2, linear_s, in.uv, 0.0).rgb * f0.z
        + textureSampleLevel(mip3, linear_s, in.uv, 0.0).rgb * f0.w
        + textureSampleLevel(mip4, linear_s, in.uv, 0.0).rgb * f1.x;
    return vec4<f32>(c, 1.0);
}
