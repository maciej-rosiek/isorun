@group(1) @binding(0) var colour_map: texture_2d<f32>;
@group(1) @binding(2) var surface_map: texture_2d<f32>;
@group(1) @binding(3) var page_sampler: sampler;
@group(1) @binding(4) var data_sampler: sampler;

struct VoxelIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) origin: vec4<f32>,
    @location(3) axis_x: vec4<f32>,
    @location(4) axis_y: vec4<f32>,
    @location(5) axis_z: vec4<f32>,
    @location(6) uv_rect: vec4<f32>,
    @location(7) tint: vec4<f32>,
    @location(8) look: vec4<f32>,
    @location(9) harm: vec4<f32>,
    @location(10) aura: vec4<f32>,
};

struct VoxelOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) view_pos: vec3<f32>,
    @location(1) world: vec3<f32>,
    @location(2) normal_world: vec3<f32>,
    @location(3) voxel: vec4<f32>,
    @location(4) uv_rect: vec4<f32>,
    @location(5) misc: vec4<f32>,
    @location(6) tint: vec4<f32>,
    @location(7) harm: vec4<f32>,
    @location(8) aura: vec4<f32>,
};

struct FragmentOut {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) emission: vec4<f32>,
};

const VOXEL_HOVER: f32 = 2.0;
const TERRAIN_RIM: f32 = 0.15;
const FIGURE_RIM: f32 = 0.4;
const STONE_GREY: vec3<f32> = vec3<f32>(0.62, 0.6, 0.57);
const RAGE_TINT: vec3<f32> = vec3<f32>(1.35, 0.7, 0.65);
const POISON_TINT: vec3<f32> = vec3<f32>(0.55, 1.15, 0.45);
const SUMMONED_LIGHT: vec3<f32> = vec3<f32>(0.25, 0.4, 0.7);
const EMBER_LIGHT: vec3<f32> = vec3<f32>(1.4, 0.5, 0.12);
const FLAMES_LIGHT: vec3<f32> = vec3<f32>(1.0, 0.42, 0.08);

@vertex
fn vs_voxel(v: VoxelIn) -> VoxelOut {
    var hover = 0.0;
    if (abs(v.look.x - VOXEL_HOVER) < 0.5) {
        hover = sin(time() * 2.2 + v.look.y) * 0.04 + 0.05;
    }
    let world = v.origin.xyz + v.axis_x.xyz * v.position.x + v.axis_y.xyz * v.position.y + v.axis_z.xyz * v.position.z + vec3<f32>(0.0, hover, 0.0);
    let normal_world = normalize(v.axis_x.xyz * v.normal.x + v.axis_y.xyz * v.normal.y + v.axis_z.xyz * v.normal.z);
    let grid = vec2<f32>(v.axis_x.w, v.axis_y.w);
    var out: VoxelOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.view_pos = (frame.view * vec4<f32>(world, 1.0)).xyz;
    out.world = world;
    out.normal_world = normal_world;
    out.voxel = vec4<f32>(v.position.xy - v.normal.xy * 0.5 / grid, grid);
    out.uv_rect = v.uv_rect;
    out.misc = vec4<f32>(v.axis_z.w, v.origin.w, v.look.y, v.look.w);
    out.tint = v.tint;
    out.harm = v.harm;
    out.aura = v.aura;
    return out;
}

fn voxel_uv(in: VoxelOut) -> vec2<f32> {
    let grid = in.voxel.zw;
    let cell = clamp(floor(in.voxel.xy * grid), vec2<f32>(0.0), ceil(grid) - 1.0);
    let middle = min(cell + 0.5, grid - 0.25) / grid;
    return mix(in.uv_rect.xy, in.uv_rect.zw, middle);
}

fn voxel_surface(in: VoxelOut, diffuse: vec3<f32>, rim: f32, gloss: f32) -> Surface {
    var s: Surface;
    let normal_world = normalize(in.normal_world);
    s.position = in.view_pos;
    s.normal = normalize((frame.view * vec4<f32>(normal_world, 0.0)).xyz);
    s.view_dir = normalize(-in.view_pos);
    s.world = in.world;
    s.normal_world = normal_world;
    s.diffuse = diffuse;
    s.specular = vec3<f32>(gloss * frame.shading1.y);
    s.shininess = mix(8.0, 160.0, gloss);
    s.rim = rim;
    s.all_round = 0.0;
    return s;
}

fn voxel_outputs(s: Surface, light: vec3<f32>) -> FragmentOut {
    var out: FragmentOut;
    out.colour = vec4<f32>(light_surface(MODEL_CLAY, s) + light, 1.0);
    out.normal = vec4<f32>(pack_normal(s.normal), 1.0);
    out.emission = vec4<f32>(light, 1.0);
    return out;
}

fn memory_of(in: VoxelOut) -> f32 {
    return mix(1.0, memory_shade(memory_cell(in.world, normalize(in.normal_world))), in.misc.w);
}

@fragment
fn fs_voxel(in: VoxelOut) -> FragmentOut {
    let at = voxel_uv(in);
    let colour = textureSampleLevel(colour_map, page_sampler, at, in.misc.x);
    let surface = textureSampleLevel(surface_map, data_sampler, at, in.misc.x);
    let shade = memory_of(in);
    let figure = in.misc.y > 0.5;
    var diffuse = colour.rgb / max(colour.a, 0.5) * shade * in.tint.rgb;
    var rim = TERRAIN_RIM;
    if (figure) {
        rim = FIGURE_RIM;
    } else {
        diffuse = terrain_colour(diffuse);
    }
    let light = colour.rgb * surface.g * frame.shading2.z * shade;
    return voxel_outputs(voxel_surface(in, diffuse, rim, surface.r), light);
}

@fragment
fn fs_voxel_actor(in: VoxelOut) -> FragmentOut {
    let at = voxel_uv(in);
    let colour = textureSampleLevel(colour_map, page_sampler, at, in.misc.x);
    let surface = textureSampleLevel(surface_map, data_sampler, at, in.misc.x);
    var harm = in.harm;
    var aura = in.aura;
    if (frame.shading3.z < 0.5) {
        harm = vec4<f32>(0.0);
        aura = vec4<f32>(0.0);
    }
    let phase = in.misc.z;
    let pulse = sin(time() * 3.0 + phase) * 0.5 + 0.5;
    let grain = interleaved_gradient_noise(in.clip.xy);
    let opacity = in.tint.w;
    let shown = opacity * mix(1.0, 0.8 + pulse * 0.2, aura.z);
    if (!(grain < shown)) {
        discard;
    }
    let base = colour.rgb / max(colour.a, 0.5);
    let grey = dot(base, vec3<f32>(0.299, 0.587, 0.114));
    var tinted = mix(base, STONE_GREY * (grey * 0.8 + 0.25), harm.x);
    tinted = mix(tinted, tinted * POISON_TINT, harm.z * (pulse * 0.4 + 0.3));
    tinted = mix(tinted, tinted * RAGE_TINT, aura.x);
    let diffuse = tinted * in.tint.rgb * memory_of(in);
    let s = voxel_surface(in, diffuse, FIGURE_RIM, mix(surface.r, 0.0, harm.x));
    let edge = pow(1.0 - saturate1(dot(s.normal, s.view_dir)), 2.0);
    var embers = 0.0;
    if (opacity < 1.0) {
        embers = smoothstep(opacity - 0.12, opacity, grain);
    }
    let light = base * surface.g * frame.shading2.z * (1.0 - harm.x)
        + EMBER_LIGHT * embers
        + FLAMES_LIGHT * harm.y * 0.5 * pulse
        + base * 0.5 * aura.y
        + SUMMONED_LIGHT * edge * aura.z;
    return voxel_outputs(s, light);
}

struct PieceIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) centre: vec4<f32>,
    @location(3) colour: vec4<f32>,
};

struct PieceOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) view_pos: vec3<f32>,
    @location(1) world: vec3<f32>,
    @location(2) normal_world: vec3<f32>,
    @location(3) colour: vec3<f32>,
};

@vertex
fn vs_piece(v: PieceIn) -> PieceOut {
    let world = v.centre.xyz + v.position * v.centre.w;
    var out: PieceOut;
    out.clip = frame.view_proj * vec4<f32>(world, 1.0);
    out.view_pos = (frame.view * vec4<f32>(world, 1.0)).xyz;
    out.world = world;
    out.normal_world = v.normal;
    out.colour = v.colour.rgb;
    return out;
}

@fragment
fn fs_piece(in: PieceOut) -> FragmentOut {
    let normal_world = normalize(in.normal_world);
    var s: Surface;
    s.position = in.view_pos;
    s.normal = normalize((frame.view * vec4<f32>(normal_world, 0.0)).xyz);
    s.view_dir = normalize(-in.view_pos);
    s.world = in.world;
    s.normal_world = normal_world;
    s.diffuse = in.colour * memory_shade(memory_cell(in.world, normal_world));
    s.specular = vec3<f32>(0.0056);
    s.shininess = 30.0;
    s.rim = FIGURE_RIM;
    s.all_round = 0.0;
    return voxel_outputs(s, vec3<f32>(0.0));
}
