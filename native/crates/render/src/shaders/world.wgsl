// The level's materials (shading.ts): the ground and the walls (`dressGround`), the sprites that
// stand (`dressSprite`), the actors (`actorMaterial`), the wall tops (`capMaterial`) and plain clay
// for models (door leaves, torches). Each writes the colour, the geometry's view normal for the
// screen effects, and the light it gives for the glow's bloom.

struct Draw {
    model: mat4x4<f32>,
    // rgb tint (the flash of a hit), w opacity (the fade of a death)
    tint: vec4<f32>,
    // motion, phase, unused, takes the memory shade
    look: vec4<f32>,
    // petrified, burning, poisoned, corroded
    harm: vec4<f32>,
    // berserk, glowing, summoned, wet
    aura: vec4<f32>,
};

@group(1) @binding(0) var colour_map: texture_2d<f32>;
@group(1) @binding(1) var normal_map: texture_2d<f32>;
@group(1) @binding(2) var surface_map: texture_2d<f32>;
@group(1) @binding(3) var page_sampler: sampler;
// the normal and surface pages': no anisotropy (pages.rs `page_bind_group`)
@group(1) @binding(4) var data_sampler: sampler;
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
    @location(0) view_pos: vec3<f32>,
    @location(1) world: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) uv_rect: vec4<f32>,
    @location(4) colour: vec3<f32>,
    @location(5) normal_view: vec3<f32>,
    @location(6) normal_world: vec3<f32>,
    @location(7) local_y: f32,
};

struct FragmentOut {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) emission: vec4<f32>,
};

const ACTOR_HOVER: f32 = 2.0;
const ACTOR_FLICKER: f32 = 3.0;

fn place(v: VertexIn, local: vec3<f32>) -> VertexOut {
    var out: VertexOut;
    let world = draw.model * vec4<f32>(local, 1.0);
    let normal_world = normalize((draw.model * vec4<f32>(v.normal, 0.0)).xyz);
    out.world = world.xyz;
    out.view_pos = (frame.view * world).xyz;
    out.clip = frame.view_proj * world;
    out.uv = v.uv;
    out.uv_rect = v.uv_rect;
    out.colour = v.colour;
    out.normal_world = normal_world;
    out.normal_view = (frame.view * vec4<f32>(normal_world, 0.0)).xyz;
    out.local_y = local.y;
    return out;
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    return place(v, v.position);
}

// An actor hovers or flickers by its label (`actorMaterial`'s positionNode).
@vertex
fn vs_actor(v: VertexIn) -> VertexOut {
    let motion = draw.look.x;
    let phase = draw.look.y;
    var hover = 0.0;
    if (abs(motion - ACTOR_HOVER) < 0.5) {
        hover = sin(time() * 2.2 + phase) * 0.04 + 0.05;
    }
    var flicker = 0.0;
    if (abs(motion - ACTOR_FLICKER) < 0.5) {
        flicker = sin(time() * 11.0 + phase) * 0.035;
    }
    return place(v, vec3<f32>(v.position.x * (1.0 + flicker), v.position.y + hover, v.position.z));
}

// The normal from the map, whose strength follows the relief setting; `flatten` lays it flat.
fn relief_normal(in: VertexOut, at: vec2<f32>, flatten: f32, face: f32, across: vec2<f32>, down: vec2<f32>) -> vec3<f32> {
    let xy = (textureSampleGrad(normal_map, data_sampler, at, across, down).rg * 2.0 - 1.0) * (1.0 - flatten);
    let z = sqrt(max(0.0, 1.0 - dot(xy, xy)));
    let map_n = vec3<f32>(xy * frame.shading0.z, z);
    let geometry = normalize(in.normal_view) * face;
    return perturb_normal(in.view_pos, geometry, in.uv, map_n, face);
}

// Where the view ray meets the tile's height (parallax occlusion), inside the quad's own rect. The
// loop has no early exit, so every pixel takes the same path.
fn parallax_uv(in: VertexOut, face: f32) -> vec2<f32> {
    let rect = in.uv_rect;
    let geometry = normalize(in.normal_view) * face;
    let view_dir = normalize(-in.view_pos);
    let view = tangent_space_view(in.view_pos, geometry, in.uv, view_dir);
    let size = rect.zw - rect.xy;
    let depth = frame.shading0.w;
    let steps = frame.shading3.x;
    let count = max(steps, 1.0);
    let step = -(view.xy / max(view.z, 0.25)) * depth * size / count;
    var at = in.uv - step * (count * 0.5);
    var layer = 0.0;
    var sunk = 0.0;
    var before = 0.0;
    var searching = 1.0;
    if (depth > 0.0 && steps > 0.0) {
        for (var i = 0; i < 16; i = i + 1) {
            if (f32(i) < count && searching > 0.0) {
                sunk = 1.0 - textureSampleLevel(normal_map, data_sampler, clamp(at, rect.xy, rect.zw), 0.0).b;
                if (sunk <= layer) {
                    searching = 0.0;
                } else {
                    before = sunk - layer;
                    at = at + step;
                    layer = layer + 1.0 / count;
                }
            }
        }
    }
    let after = sunk - layer;
    let between = saturate1(after / min(after - before, -1e-4));
    var t = 0.0;
    if (layer > 0.0) {
        t = between;
    }
    return clamp(mix(at, at - step, t), rect.xy, rect.zw);
}

const SUNKEN_DEPTH: f32 = 0.35;
const SUNKEN_RIPPLE: f32 = 0.012;
const SUNKEN_SCALE: f32 = 9.0;
const SUNKEN_SPEED: f32 = 0.8;

// Where the ground under water is seen: lower by the water's depth, along the view, bent by ripples.
fn under_water(in: VertexOut, at: vec2<f32>, submerged: f32, face: f32) -> vec2<f32> {
    let liquid_depth = frame.shading3.y;
    if (submerged * liquid_depth <= 0.001) {
        return at;
    }
    let rect = in.uv_rect;
    let size = rect.zw - rect.xy;
    let geometry = normalize(in.normal_view) * face;
    let view = tangent_space_view(in.view_pos, geometry, in.uv, normalize(-in.view_pos));
    let sink = -(view.xy / max(view.z, 0.25)) * submerged * liquid_depth * SUNKEN_DEPTH;
    let p = in.world.xz * SUNKEN_SCALE;
    let wave = vec2<f32>(mx_noise_float(vec3<f32>(p, time() * SUNKEN_SPEED)), mx_noise_float(vec3<f32>(p + 17.0, time() * SUNKEN_SPEED)));
    let shifted = (at - rect.xy) / size + sink + wave * SUNKEN_RIPPLE * submerged * liquid_depth;
    return rect.xy + fract(shifted) * size;
}

const PAGE_SOURCE_TEXELS: f32 = 1024.0;

// The light a texel gives: its colour where it glows, breathing, and glints where it sparkles.
fn emission(colour: vec3<f32>, surface: vec4<f32>, at: vec2<f32>, phase: f32) -> vec3<f32> {
    let pulse = 1.0 + sin(time() * 1.7 + phase) * 0.15;
    let glow = colour * surface.g * frame.shading2.z * pulse;
    var glint = 0.0;
    if (surface.b > 0.0 && frame.shading2.w > 0.0) {
        glint = smoothstep(0.62, 1.0, mx_noise_float(vec3<f32>(at * (PAGE_SOURCE_TEXELS / 1.5), time() * 1.3)) * 0.5 + 0.5);
    }
    return glow + vec3<f32>(glint * surface.b * frame.shading2.w);
}

fn gloss_specular(gloss: f32) -> vec3<f32> {
    return vec3<f32>(gloss * frame.shading1.y);
}

fn gloss_shininess(gloss: f32) -> f32 {
    return mix(8.0, 160.0, gloss);
}

fn surface_of(in: VertexOut, normal: vec3<f32>, diffuse: vec3<f32>) -> Surface {
    var s: Surface;
    s.position = in.view_pos;
    s.normal = normal;
    s.view_dir = normalize(-in.view_pos);
    s.world = in.world;
    s.normal_world = normalize(in.normal_world);
    s.diffuse = diffuse;
    s.specular = vec3<f32>(0.0056);
    s.shininess = 30.0;
    s.rim = 0.0;
    return s;
}

fn outputs(colour: vec3<f32>, alpha: f32, in: VertexOut, face: f32, light: vec3<f32>) -> FragmentOut {
    var out: FragmentOut;
    out.colour = vec4<f32>(colour, alpha);
    out.normal = vec4<f32>(pack_normal(normalize(in.normal_view) * face), 1.0);
    out.emission = vec4<f32>(light, 1.0);
    return out;
}

const CAVITY_SPREAD: f32 = 8.0;
const CAVITY_SHARPNESS: f32 = 6.0;
const CAVITY_DARK: f32 = 0.45;
const EDGE_LIGHT: f32 = 0.35;
const PUDDLE_LEVEL: f32 = 0.55;
const PUDDLE_EDGE: f32 = 0.04;
const WET_DARK: f32 = 0.55;
const WET_GLOSS: f32 = 0.4;
const GROUND_RIM: f32 = 0.15;

// The ground and the walls: the tile seen through its height, dirt in its hollows and wear on its
// edges, each cell a little different, wet near water with puddles, glossy and glowing where its
// surface says; darker on a remembered cell.
@fragment
fn fs_ground(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {
    return ground(in, front, 0.5);
}

// crawl's see-through marks on the ground (the travel exclusion), blended over the floor under them.
@fragment
fn fs_mark(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {
    return ground(in, front, 0.01);
}

fn ground(in: VertexOut, front: bool, least_alpha: f32) -> FragmentOut {
    let face = select(-1.0, 1.0, front);
    let across = dpdx(in.uv);
    let down = dfdy2(in.uv);
    let cell = memory_cell(in.world, normalize(in.normal_world));
    let shade = memory_shade(cell);
    let wet_map = wetness_at(in.world);
    let sunk = under_water(in, parallax_uv(in, face), wet_map.g, face);
    let colour = textureSampleGrad(colour_map, page_sampler, sunk, across, down);
    if (colour.a < least_alpha) {
        discard;
    }
    let surface = textureSampleGrad(surface_map, data_sampler, sunk, across, down);
    let height = textureSampleGrad(normal_map, data_sampler, sunk, across, down).b;
    let around = textureSampleGrad(normal_map, data_sampler, sunk, across * CAVITY_SPREAD, down * CAVITY_SPREAD).b;
    let cavity = frame.shading1.w;
    let hollow = saturate1((around - height) * CAVITY_SHARPNESS) * cavity;
    let ridge = saturate1((height - around) * CAVITY_SHARPNESS) * cavity;
    let cell_hash = hash1(cell.x * 127.1 + cell.y * 311.7);
    let varied = 1.0 + (cell_hash - 0.5) * 2.0 * frame.shading2.x;
    let wet = saturate1(wet_map.r * frame.shading2.y);
    let upward = smoothstep(0.5, 0.9, normalize(in.normal_world).y);
    let puddle = smoothstep(0.0, PUDDLE_EDGE, wet * PUDDLE_LEVEL - height + 0.5 - PUDDLE_LEVEL / 2.0) * upward;
    let shaded = colour.rgb * (1.0 - hollow * CAVITY_DARK) * (1.0 + ridge * EDGE_LIGHT) * varied * mix(1.0, WET_DARK, max(wet * 0.6, puddle));
    let diffuse = shaded * shade * in.colour;
    let normal = relief_normal(in, sunk, puddle, face, across, down);
    var s = surface_of(in, normal, diffuse);
    let gloss = max(surface.r, max(wet * WET_GLOSS, puddle));
    s.specular = gloss_specular(gloss);
    s.shininess = gloss_shininess(gloss);
    s.rim = GROUND_RIM;
    let light = emission(colour.rgb, surface, sunk, cell_hash * 6.283) * shade;
    return outputs(light_surface(MODEL_CLAY, s) + light, colour.a, in, face, light);
}

// The sprites that stand in the level (items, features): lit with the sprite light, glossy and
// glowing where their surface says, with a rim of light.
@fragment
fn fs_sprite(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {
    let face = select(-1.0, 1.0, front);
    let colour = textureSample(colour_map, page_sampler, in.uv);
    if (colour.a < 0.5) {
        discard;
    }
    let surface = textureSample(surface_map, data_sampler, in.uv);
    let shade = memory_shade(memory_cell(in.world, normalize(in.normal_world)));
    let diffuse = colour.rgb * frame.shading1.x * shade * in.colour;
    let normal = relief_normal(in, in.uv, 0.0, face, dpdx(in.uv), dfdy2(in.uv));
    var s = surface_of(in, normal, diffuse);
    s.specular = gloss_specular(surface.r);
    s.shininess = gloss_shininess(surface.r);
    s.rim = 1.0;
    s.all_round = 1.0;
    let light = emission(colour.rgb, surface, in.uv, 0.0) * shade;
    return outputs(light_surface(MODEL_CLAY, s) + light, colour.a, in, face, light);
}

const STONE: vec3<f32> = vec3<f32>(0.62, 0.6, 0.57);
const POISON: vec3<f32> = vec3<f32>(0.55, 1.15, 0.45);
const RUST: vec3<f32> = vec3<f32>(0.55, 0.5, 0.3);
const RAGE: vec3<f32> = vec3<f32>(1.35, 0.7, 0.65);
const RAGE_GLOW: vec3<f32> = vec3<f32>(0.7, 0.06, 0.02);
const SUMMONED_GLOW: vec3<f32> = vec3<f32>(0.25, 0.4, 0.7);
const EMBERS: vec3<f32> = vec3<f32>(1.4, 0.5, 0.12);
const FLAME_COLOUR: vec3<f32> = vec3<f32>(1.0, 0.42, 0.08);
const SEE_THROUGH: f32 = 0.55;
const SUMMONED_SHOWN: f32 = 0.8;
const DISSOLVE_EDGE: f32 = 0.12;

// One sprite material for every actor: the flash, the fade, the motion and the statuses come with
// the draw. The fade is a dissolve, which stays opaque; a see-through monster is drawn in part.
@fragment
fn fs_actor(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {
    let face = select(-1.0, 1.0, front);
    let colour = textureSample(colour_map, page_sampler, in.uv);
    let surface = textureSample(surface_map, data_sampler, in.uv);
    let opacity = draw.tint.w;
    let phase = draw.look.y;
    var harm = draw.harm;
    var aura = draw.aura;
    if (frame.shading3.z < 0.5) {
        harm = vec4<f32>(0.0);
        aura = vec4<f32>(0.0);
    }
    let pulse = sin(time() * 3.0 + phase) * 0.5 + 0.5;
    let grain = interleaved_gradient_noise(in.clip.xy);
    let shown = opacity * mix(1.0, SEE_THROUGH, surface.a) * mix(1.0, SUMMONED_SHOWN + pulse * (1.0 - SUMMONED_SHOWN), aura.z);
    if (colour.a < 0.5 || !(grain < shown)) {
        discard;
    }
    let texel = in.uv * PAGE_SOURCE_TEXELS;
    let grey = dot(colour.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let stone = STONE * (grey * 0.8 + 0.25);
    var rust = 0.0;
    if (harm.w > 0.0) {
        rust = smoothstep(0.55, 0.75, mx_noise_float(vec3<f32>(texel * 0.35, 0.0)) * 0.5 + 0.5) * harm.w;
    }
    var tinted = mix(colour.rgb, stone, harm.x);
    tinted = mix(tinted, tinted * POISON, harm.z * (pulse * 0.4 + 0.3));
    tinted = mix(tinted, tinted * RUST, rust);
    tinted = mix(tinted, tinted * RAGE, aura.x);
    tinted = mix(tinted, tinted * (WET_DARK + 0.2), aura.w);
    let diffuse = tinted * frame.shading1.x * draw.tint.rgb * in.colour;
    let normal = relief_normal(in, in.uv, 0.0, face, dpdx(in.uv), dfdy2(in.uv));
    var s = surface_of(in, normal, diffuse);
    let gloss = mix(max(surface.r, aura.w * WET_GLOSS), 0.0, harm.x);
    s.specular = gloss_specular(gloss);
    s.shininess = gloss_shininess(gloss);
    s.rim = 1.0;
    s.all_round = 1.0;
    var flames = 0.0;
    if (harm.y > 0.0) {
        let p = texel / 32.0 * vec2<f32>(7.0, 4.0) - vec2<f32>(0.0, time() * 1.6);
        let flame_noise = mx_noise_float(vec3<f32>(p, phase)) * 0.5 + 0.5;
        flames = smoothstep(0.6, 0.8, flame_noise + 0.25 - in.local_y * 0.5);
    }
    let edge = pow(1.0 - saturate1(dot(normal, normalize(-in.view_pos))), 2.0);
    var embers = 0.0;
    if (opacity < 1.0) {
        embers = smoothstep(opacity - DISSOLVE_EDGE, opacity, grain);
    }
    let light = emission(colour.rgb, surface, in.uv, phase) * (1.0 - harm.x)
        + EMBERS * embers
        + FLAME_COLOUR * flames * harm.y * 0.9
        + RAGE_GLOW * edge * aura.x * (pulse * 0.5 + 0.5)
        + colour.rgb * 0.5 * aura.y
        + SUMMONED_GLOW * edge * aura.z;
    return outputs(light_surface(MODEL_CLAY, s) + light, 1.0, in, face, light);
}

// The wall tops: a faint stone grain under the vertex colours, darker on a remembered cell.
@fragment
fn fs_cap(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {
    let face = select(-1.0, 1.0, front);
    let grain = textureSample(colour_map, page_sampler, in.uv);
    let shade = memory_shade(memory_cell(in.world, normalize(in.normal_world)));
    let normal = normalize(in.normal_view) * face;
    let s = surface_of(in, normal, grain.rgb * shade * in.colour);
    return outputs(light_surface(MODEL_CLAY, s), 1.0, in, face, vec3<f32>(0.0));
}

// Plain clay with a texture: door leaves and the torch models.
@fragment
fn fs_clay(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {
    let face = select(-1.0, 1.0, front);
    let colour = textureSample(colour_map, page_sampler, in.uv);
    let normal = normalize(in.normal_view) * face;
    var shade = 1.0;
    if (draw.look.w > 0.5) {
        shade = memory_shade(memory_cell(in.world, normalize(in.normal_world)));
    }
    let s = surface_of(in, normal, colour.rgb * in.colour * draw.tint.rgb * shade);
    return outputs(light_surface(MODEL_CLAY, s), 1.0, in, face, vec3<f32>(0.0));
}
