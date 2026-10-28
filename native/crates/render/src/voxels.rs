use crate::gpu::{Blend, Gpu};
use crate::layers::{layer_pipeline, DynamicBuffer};
use crate::mesh::Point3;
use crate::pages::Pages;
use crate::TileSource;
use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Mat4, Vec3};
use isorun_scene::{CellKind, Scene, TileId, TileRect, Visibility};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub const WALL_LEVELS: i32 = 3;
pub const FLOOR_LEVELS: i32 = 1;
pub const FIGURE_LEVELS: i32 = 4;
const OPAQUE: u8 = 128;
const OUTLINE: f32 = 40.0;
const DEBRIS_GRAVITY: f64 = 9.0;
const DEBRIS_BOUNCE: f64 = 0.35;
const DEBRIS_FRICTION: f64 = 0.6;
const DEBRIS_SECONDS: f64 = 2.4;
const DEBRIS_SHRINK_SECONDS: f64 = 0.6;
const FIGURE_PIECES: u32 = 16;
const WALL_PIECES: u32 = 6;
const MOST_BROKEN_WALLS: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VoxelKind {
    Wall,
    Floor,
    Figure,
}

impl VoxelKind {
    pub fn levels(self) -> i32 {
        match self {
            VoxelKind::Wall => WALL_LEVELS,
            VoxelKind::Floor => FLOOR_LEVELS,
            VoxelKind::Figure => FIGURE_LEVELS,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct VoxelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

impl VoxelVertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout { array_stride: stride_of::<VoxelVertex>(), step_mode: wgpu::VertexStepMode::Vertex, attributes: &Self::ATTRIBUTES }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct VoxelInstance {
    pub origin: [f32; 4],
    pub axis_x: [f32; 4],
    pub axis_y: [f32; 4],
    pub axis_z: [f32; 4],
    pub uv_rect: [f32; 4],
    pub tint: [f32; 4],
    pub look: [f32; 4],
    pub harm: [f32; 4],
    pub aura: [f32; 4],
}

impl VoxelInstance {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
        2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4, 9 => Float32x4, 10 => Float32x4
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout { array_stride: stride_of::<VoxelInstance>(), step_mode: wgpu::VertexStepMode::Instance, attributes: &Self::ATTRIBUTES }
    }

    pub fn moved(&self, model: Mat4) -> VoxelInstance {
        let point = |v: [f32; 4]| model.transform_point3(Vec3::new(v[0], v[1], v[2]));
        let vector = |v: [f32; 4]| model.transform_vector3(Vec3::new(v[0], v[1], v[2]));
        let with = |v: Vec3, w: f32| [v.x, v.y, v.z, w];
        VoxelInstance {
            origin: with(point(self.origin), self.origin[3]),
            axis_x: with(vector(self.axis_x), self.axis_x[3]),
            axis_y: with(vector(self.axis_y), self.axis_y[3]),
            axis_z: with(vector(self.axis_z), self.axis_z[3]),
            ..*self
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PieceInstance {
    pub centre: [f32; 4],
    pub colour: [f32; 4],
}

impl PieceInstance {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x4];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout { array_stride: stride_of::<PieceInstance>(), step_mode: wgpu::VertexStepMode::Instance, attributes: &Self::ATTRIBUTES }
    }
}

fn stride_of<T>() -> u64 {
    u64::try_from(std::mem::size_of::<T>()).unwrap_or(0)
}

fn to_f32(v: usize) -> f32 {
    u16::try_from(v).map(f32::from).unwrap_or(f32::MAX)
}

fn level_f32(v: i32) -> f32 {
    i16::try_from(v).map(f32::from).unwrap_or(0.0)
}

fn index_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Columns {
    pub across: usize,
    pub down: usize,
    pub levels: i32,
    pub closed: bool,
    pub spans: Vec<Option<(i32, i32)>>,
    pub x_edges: Vec<f32>,
    pub y_edges: Vec<f32>,
}

impl Columns {
    fn span(&self, x: usize, y: usize) -> Option<(i32, i32)> {
        self.spans[y * self.across + x]
    }

    fn neighbour(&self, x: usize, y: usize, dx: i32, dy: i32) -> Option<(i32, i32)> {
        let nx = x.checked_add_signed(isize::try_from(dx).ok()?)?;
        let ny = y.checked_add_signed(isize::try_from(dy).ok()?)?;
        if nx >= self.across || ny >= self.down {
            return None;
        }
        self.span(nx, ny)
    }
}

fn edges(count: usize, step: usize, size: usize) -> Vec<f32> {
    (0..=count).map(|i| to_f32((i * step).min(size)) / to_f32(size.max(1))).collect()
}

pub fn columns(rgba: &[u8], width: usize, rows: usize, step: usize, kind: VoxelKind) -> Columns {
    let step = step.max(1);
    let across = width.div_ceil(step);
    let down = rows.div_ceil(step);
    let mut solid = vec![false; across * down];
    let mut bright = vec![0.0f32; across * down];
    for y in 0..down {
        for x in 0..across {
            let (mut sum, mut opaque, mut all) = (0.0, 0usize, 0usize);
            for ty in y * step..((y + 1) * step).min(rows) {
                for tx in x * step..((x + 1) * step).min(width) {
                    let i = (ty * width + tx) * 4;
                    let Some(p) = rgba.get(i..i + 4) else { continue };
                    all += 1;
                    if p[3] >= OPAQUE {
                        opaque += 1;
                        sum += 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]);
                    }
                }
            }
            solid[y * across + x] = opaque * 2 >= all.max(1);
            bright[y * across + x] = if opaque > 0 { sum / to_f32(opaque) } else { 0.0 };
        }
    }
    let levels = kind.levels();
    if kind == VoxelKind::Figure {
        let rim = edge_distance(&solid, across, down);
        for (i, s) in solid.iter_mut().enumerate() {
            *s = *s && !(rim[i] == 1 && bright[i] < OUTLINE);
        }
    } else {
        bright = blurred(&bright, across, down);
    }
    let lit: Vec<f32> = bright.iter().zip(&solid).filter(|(_, s)| **s).map(|(b, _)| *b).collect();
    let mean = lit.iter().sum::<f32>() / to_f32(lit.len().max(1));
    let spread = (lit.iter().map(|b| (b - mean) * (b - mean)).sum::<f32>() / to_f32(lit.len().max(1))).sqrt().max(4.0);
    let standing = |i: usize| (bright[i] - mean) / spread;
    let spans: Vec<Option<(i32, i32)>> = match kind {
        VoxelKind::Wall | VoxelKind::Floor => {
            let mut heights: Vec<i32> = (0..across * down).map(|i| terrain_level(kind, standing(i))).collect();
            despeckle(&mut heights, across, down);
            despeckle(&mut heights, across, down);
            heights.into_iter().map(|h| Some((-levels, h - levels))).collect()
        }
        VoxelKind::Figure => {
            let inside = edge_distance(&solid, across, down);
            (0..across * down)
                .map(|i| {
                    solid[i].then(|| {
                        let dome = inside[i].min(levels);
                        let raised = if standing(i) > 0.8 {
                            1
                        } else if standing(i) < -1.2 {
                            -1
                        } else {
                            0
                        };
                        let h = (dome + raised).clamp(1, levels);
                        (-h, h)
                    })
                })
                .collect()
        }
    };
    Columns { across, down, levels, closed: kind == VoxelKind::Figure, spans, x_edges: edges(across, step, width), y_edges: edges(down, step, rows) }
}

fn terrain_level(kind: VoxelKind, standing: f32) -> i32 {
    match kind {
        VoxelKind::Floor => i32::from(standing >= -1.0),
        _ => {
            let sunk = ((0.3 - standing) / 1.8).clamp(0.0, 1.0);
            nearest_level(level_f32(WALL_LEVELS) * (1.0 - sunk), WALL_LEVELS)
        }
    }
}

fn nearest_level(target: f32, levels: i32) -> i32 {
    (0..=levels).min_by(|a, b| (level_f32(*a) - target).abs().total_cmp(&(level_f32(*b) - target).abs())).unwrap_or(0)
}

fn blurred(bright: &[f32], across: usize, down: usize) -> Vec<f32> {
    let mut out = vec![0.0; bright.len()];
    for y in 0..down {
        for x in 0..across {
            let (mut sum, mut n) = (0.0, 0usize);
            for ny in y.saturating_sub(1)..(y + 2).min(down) {
                for nx in x.saturating_sub(1)..(x + 2).min(across) {
                    sum += bright[ny * across + nx];
                    n += 1;
                }
            }
            out[y * across + x] = sum / to_f32(n.max(1));
        }
    }
    out
}

fn despeckle(heights: &mut [i32], across: usize, down: usize) {
    let before = heights.to_vec();
    for y in 1..down.saturating_sub(1) {
        for x in 1..across.saturating_sub(1) {
            let i = y * across + x;
            let around = [before[i - 1], before[i + 1], before[i - across], before[i + across]];
            if around.iter().all(|h| *h == around[0]) && around[0] != before[i] {
                heights[i] = around[0];
            }
        }
    }
}

fn edge_distance(solid: &[bool], across: usize, down: usize) -> Vec<i32> {
    let big = i32::try_from(across + down).unwrap_or(i32::MAX / 2);
    let mut d: Vec<i32> = solid.iter().map(|s| if *s { big } else { 0 }).collect();
    let at = |d: &[i32], x: Option<usize>, y: Option<usize>| match (x, y) {
        (Some(x), Some(y)) if x < across && y < down => d[y * across + x],
        _ => 0,
    };
    for y in 0..down {
        for x in 0..across {
            let i = y * across + x;
            if d[i] > 0 {
                d[i] = d[i].min(at(&d, x.checked_sub(1), Some(y)) + 1).min(at(&d, Some(x), y.checked_sub(1)) + 1);
            }
        }
    }
    for y in (0..down).rev() {
        for x in (0..across).rev() {
            let i = y * across + x;
            if d[i] > 0 {
                d[i] = d[i].min(at(&d, Some(x + 1), Some(y)) + 1).min(at(&d, Some(x), Some(y + 1)) + 1);
            }
        }
    }
    d
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelMesh {
    pub vertices: Vec<VoxelVertex>,
    pub indices: Vec<u32>,
}

impl VoxelMesh {
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    fn quad(&mut self, corners: [[f32; 3]; 4], normal: [f32; 3]) {
        let base = index_u32(self.vertices.len());
        for position in corners {
            self.vertices.push(VoxelVertex { position, normal });
        }
        self.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

pub fn mesh(c: &Columns) -> VoxelMesh {
    let mut m = VoxelMesh::default();
    let z = |level: i32| level_f32(level) / level_f32(c.levels.max(1));
    let tops: Vec<Option<i32>> = c.spans.iter().map(|s| s.map(|s| s.1)).collect();
    for (x0, y0, x1, y1, h) in rectangles(&tops, c.across, c.down) {
        let (ax, bx, ay, by, hz) = (c.x_edges[x0], c.x_edges[x1], c.y_edges[y0], c.y_edges[y1], z(h));
        m.quad([[ax, ay, hz], [bx, ay, hz], [bx, by, hz], [ax, by, hz]], [0.0, 0.0, 1.0]);
    }
    if c.closed {
        let backs: Vec<Option<i32>> = c.spans.iter().map(|s| s.map(|s| s.0)).collect();
        for (x0, y0, x1, y1, h) in rectangles(&backs, c.across, c.down) {
            let (ax, bx, ay, by, hz) = (c.x_edges[x0], c.x_edges[x1], c.y_edges[y0], c.y_edges[y1], z(h));
            m.quad([[ax, ay, hz], [ax, by, hz], [bx, by, hz], [bx, ay, hz]], [0.0, 0.0, -1.0]);
        }
    }
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let mut runs: Vec<(usize, i32, i32, usize)> = Vec::new();
        for y in 0..c.down {
            for x in 0..c.across {
                let Some(own) = c.span(x, y) else { continue };
                let (line, along) = if dx != 0 { (if dx > 0 { x + 1 } else { x }, y) } else { (if dy > 0 { y + 1 } else { y }, x) };
                for (a, b) in exposed(own, c.neighbour(x, y, dx, dy)) {
                    runs.push((line, a, b, along));
                }
            }
        }
        runs.sort_unstable();
        let mut i = 0;
        while i < runs.len() {
            let (line, a, b, start) = runs[i];
            let mut end = start;
            while i + 1 < runs.len() && runs[i + 1] == (line, a, b, end + 1) {
                end += 1;
                i += 1;
            }
            i += 1;
            let (za, zb) = (z(a), z(b));
            if dx != 0 {
                let (lx, y0, y1) = (c.x_edges[line], c.y_edges[start], c.y_edges[end + 1]);
                let n = [level_f32(dx), 0.0, 0.0];
                m.quad([[lx, y0, zb], [lx, y1, zb], [lx, y1, za], [lx, y0, za]], n);
            } else {
                let (ly, x0, x1) = (c.y_edges[line], c.x_edges[start], c.x_edges[end + 1]);
                let n = [0.0, level_f32(dy), 0.0];
                m.quad([[x0, ly, zb], [x1, ly, zb], [x1, ly, za], [x0, ly, za]], n);
            }
        }
    }
    m
}

fn exposed(own: (i32, i32), next: Option<(i32, i32)>) -> Vec<(i32, i32)> {
    let (lo, hi) = own;
    match next {
        None => vec![(lo, hi)],
        Some((nlo, nhi)) => {
            let mut out = Vec::new();
            if lo < nlo {
                out.push((lo, hi.min(nlo)));
            }
            if hi > nhi {
                out.push((lo.max(nhi), hi));
            }
            out
        }
    }
    .into_iter()
    .filter(|(a, b)| a < b)
    .collect()
}

fn rectangles(values: &[Option<i32>], across: usize, down: usize) -> Vec<(usize, usize, usize, usize, i32)> {
    let mut taken = vec![false; values.len()];
    let mut out = Vec::new();
    for y in 0..down {
        for x in 0..across {
            let i = y * across + x;
            let Some(v) = values[i].filter(|_| !taken[i]) else { continue };
            let same = |xx: usize, yy: usize, taken: &[bool]| {
                let j = yy * across + xx;
                !taken[j] && values[j] == Some(v)
            };
            let mut x1 = x + 1;
            while x1 < across && same(x1, y, &taken) {
                x1 += 1;
            }
            let mut y1 = y + 1;
            while y1 < down && (x..x1).all(|xx| same(xx, y1, &taken)) {
                y1 += 1;
            }
            for yy in y..y1 {
                for xx in x..x1 {
                    taken[yy * across + xx] = true;
                }
            }
            out.push((x, y, x1, y1, v));
        }
    }
    out
}

pub struct VoxelGpuMesh {
    id: u64,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

impl VoxelGpuMesh {
    fn new(device: &wgpu::Device, mesh: &VoxelMesh, id: u64) -> Self {
        VoxelGpuMesh {
            id,
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("voxels"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("voxel indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            count: index_u32(mesh.indices.len()),
        }
    }

    fn triangles(&self) -> u64 {
        u64::from(self.count / 3)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MeshKey {
    atlas: String,
    sx: u32,
    sy: u32,
    w: u32,
    rows: u32,
    step: u32,
    kind: VoxelKind,
}

#[derive(Clone)]
pub struct VoxelPart {
    mesh: Arc<VoxelGpuMesh>,
    page: u32,
    tile: TileId,
    rows: u32,
    pub glows: bool,
    pub instance: VoxelInstance,
}

struct VoxelDraw {
    mesh: Arc<VoxelGpuMesh>,
    page: u32,
    first: u32,
    count: u32,
}

pub struct VoxelChunk {
    instances: wgpu::Buffer,
    draws: Vec<VoxelDraw>,
}

fn grouped(mut parts: Vec<VoxelPart>) -> (Vec<VoxelInstance>, Vec<VoxelDraw>) {
    parts.sort_by_key(|p| (p.page, p.mesh.id));
    let mut instances = Vec::with_capacity(parts.len());
    let mut draws: Vec<VoxelDraw> = Vec::new();
    for p in parts {
        let at = index_u32(instances.len());
        instances.push(p.instance);
        match draws.last_mut() {
            Some(d) if d.mesh.id == p.mesh.id && d.page == p.page => d.count += 1,
            _ => draws.push(VoxelDraw { mesh: p.mesh, page: p.page, first: at, count: 1 }),
        }
    }
    (instances, draws)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VoxelStats {
    pub draws: u32,
    pub triangles: u64,
    pub instances: u32,
    pub meshes: usize,
    pub pieces: usize,
}

#[derive(Clone, Copy, Debug)]
struct Piece {
    at: DVec3,
    velocity: DVec3,
    size: f64,
    colour: [f32; 3],
    age: f64,
}

pub struct ActorFigure<'a> {
    pub id: i64,
    pub key: u64,
    pub model: Mat4,
    pub uniform: crate::uniforms::DrawUniform,
    pub layers: &'a [FigureLayer],
    pub fallen: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FigureLayer {
    pub tile: TileId,
    pub corners: [Point3; 4],
    pub rows: f32,
}

#[derive(Clone, Copy, PartialEq)]
struct PipelineKey {
    samples: u32,
    cell_lights: u32,
    materials: bool,
}

struct Pipelines {
    key: PipelineKey,
    terrain: wgpu::RenderPipeline,
    actors: wgpu::RenderPipeline,
    pieces: wgpu::RenderPipeline,
}

pub struct VoxelLayer {
    wanted: bool,
    cells: u32,
    meshes: HashMap<MeshKey, Arc<VoxelGpuMesh>>,
    next_id: u64,
    modules: HashMap<bool, wgpu::ShaderModule>,
    pipelines: Option<Pipelines>,
    figures: HashMap<u64, Vec<VoxelPart>>,
    actor_instances: Option<DynamicBuffer>,
    actor_draws: Vec<VoxelDraw>,
    shattered: HashSet<i64>,
    pieces: Vec<Piece>,
    piece_instances: Option<DynamicBuffer>,
    cube: Option<VoxelGpuMesh>,
    fresh_level: bool,
    lift: f32,
    random: Box<dyn FnMut() -> f32 + Send + Sync>,
    pub stats: VoxelStats,
}

impl Default for VoxelLayer {
    fn default() -> Self {
        VoxelLayer {
            wanted: false,
            cells: 16,
            meshes: HashMap::new(),
            next_id: 0,
            modules: HashMap::new(),
            pipelines: None,
            figures: HashMap::new(),
            actor_instances: None,
            actor_draws: Vec::new(),
            shattered: HashSet::new(),
            pieces: Vec::new(),
            piece_instances: None,
            cube: None,
            fresh_level: true,
            lift: 1.0,
            random: Box::new(crate::renderer::seeded(29)),
            stats: VoxelStats::default(),
        }
    }
}

impl VoxelLayer {
    pub fn configure(&mut self, wanted: bool, cells: u32) {
        let cells = cells.clamp(4, 32);
        if wanted == self.wanted && cells == self.cells {
            return;
        }
        self.wanted = wanted;
        self.cells = cells;
        self.meshes.clear();
        self.figures.clear();
        self.actor_draws.clear();
        self.pieces.clear();
        if !wanted {
            self.actor_instances = None;
            self.piece_instances = None;
            self.stats = VoxelStats::default();
        }
    }

    pub fn on(&self) -> bool {
        self.wanted
    }

    pub fn key(&self) -> u32 {
        if self.wanted {
            self.cells
        } else {
            0
        }
    }

    pub fn new_level(&mut self) {
        self.pieces.clear();
        self.shattered.clear();
        self.figures.clear();
        self.fresh_level = true;
    }

    pub fn animating(&self) -> bool {
        !self.pieces.is_empty()
    }

    pub fn pages_changed(&mut self) {
        self.figures.clear();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn part(
        &mut self,
        gpu: &Gpu,
        pages: &mut Pages,
        tiles: &dyn TileSource,
        tile: TileId,
        kind: VoxelKind,
        corners: [Point3; 4],
        rows: Option<f32>,
        depth: f32,
        shade: f32,
    ) -> Option<VoxelPart> {
        let rect = tiles.tile(tile)?;
        self.lift = pages.paint().lift;
        let placed = pages.for_tile(&gpu.device, &gpu.page_layout, &gpu.page_sampler, tiles, &rect)?;
        let shown = rows.map_or(rect.h, |r| (1..=rect.h).find(|n| texels_f32(*n) >= r - 0.01).unwrap_or(rect.h));
        let step = (rect.cell / self.cells).max(1);
        let mesh = self.mesh_for(&gpu.device, tiles, &rect, shown, step, kind)?;
        let texels = pages.page_texels();
        let uv_rect = [placed.sx / texels, placed.sy / texels, (placed.sx + texels_f32(rect.w)) / texels, (placed.sy + texels_f32(shown)) / texels];
        let [tl, tr, _, bl] = corners.map(Vec3::from);
        let (across, down) = (tr - tl, bl - tl);
        let out = down.cross(across).normalize_or_zero() * depth;
        let scale = crate::enhance::upscale_factor(pages.paint().upscale) * step;
        let figure = if kind == VoxelKind::Figure { 1.0 } else { 0.0 };
        let instance = VoxelInstance {
            origin: [tl.x, tl.y, tl.z, figure],
            axis_x: [across.x, across.y, across.z, texels_f32(rect.w) / texels_f32(step)],
            axis_y: [down.x, down.y, down.z, texels_f32(shown) / texels_f32(step)],
            axis_z: [out.x, out.y, out.z, texels_f32(scale).log2()],
            uv_rect,
            tint: [shade, shade, shade, 1.0],
            look: [0.0, 0.0, 0.0, 1.0],
            harm: [0.0; 4],
            aura: [0.0; 4],
        };
        Some(VoxelPart { mesh, page: placed.page, tile, rows: shown, glows: placed.label.is_some_and(|l| l.glow > 0), instance })
    }

    fn mesh_for(&mut self, device: &wgpu::Device, tiles: &dyn TileSource, rect: &TileRect, rows: u32, step: u32, kind: VoxelKind) -> Option<Arc<VoxelGpuMesh>> {
        let key = MeshKey { atlas: rect.atlas.clone(), sx: rect.sx, sy: rect.sy, w: rect.w, rows, step, kind };
        if let Some(m) = self.meshes.get(&key) {
            return Some(m.clone());
        }
        let pixels = tile_pixels(tiles, rect, rows)?;
        let built = mesh(&columns(&pixels, usize_of(rect.w), usize_of(rows), usize_of(step), kind));
        if built.indices.is_empty() {
            return None;
        }
        self.next_id += 1;
        let m = Arc::new(VoxelGpuMesh::new(device, &built, self.next_id));
        self.meshes.insert(key, m.clone());
        Some(m)
    }

    pub fn chunk(&self, device: &wgpu::Device, parts: Vec<VoxelPart>) -> Option<VoxelChunk> {
        if parts.is_empty() {
            return None;
        }
        let (instances, draws) = grouped(parts);
        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("voxel instances"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Some(VoxelChunk { instances, draws })
    }

    pub fn actors(&mut self, gpu: &Gpu, pages: &mut Pages, tiles: &dyn TileSource, figures: &[ActorFigure], depth: f32) {
        let mut parts = Vec::new();
        for f in figures {
            if !self.figures.contains_key(&f.key) {
                let built: Vec<VoxelPart> = f
                    .layers
                    .iter()
                    .enumerate()
                    .filter_map(|(i, l)| self.part(gpu, pages, tiles, l.tile, VoxelKind::Figure, l.corners, Some(l.rows), layer_depth(depth, i), 1.0))
                    .collect();
                self.figures.insert(f.key, built);
            }
            let Some(own) = self.figures.get(&f.key) else { continue };
            let u = &f.uniform;
            let placed: Vec<VoxelPart> = own
                .iter()
                .map(|p| {
                    let mut instance = p.instance.moved(f.model);
                    instance.origin[3] = 2.0;
                    (instance.tint, instance.look, instance.harm, instance.aura) = (u.tint, u.look, u.harm, u.aura);
                    VoxelPart { instance, ..p.clone() }
                })
                .collect();
            if f.fallen {
                if self.shattered.insert(f.id) {
                    for p in &placed {
                        self.shatter(tiles, p);
                    }
                }
                continue;
            }
            parts.extend(placed);
        }
        let (instances, draws) = grouped(parts);
        self.actor_draws = draws;
        if !instances.is_empty() {
            let buffer = self.actor_instances.get_or_insert_with(|| DynamicBuffer::new(gpu, "voxel actors", wgpu::BufferUsages::VERTEX, 4096));
            buffer.write(gpu, bytemuck::cast_slice(&instances));
        }
    }

    fn shatter(&mut self, tiles: &dyn TileSource, p: &VoxelPart) {
        let Some(rect) = tiles.tile(p.tile) else { return };
        let Some(pixels) = tile_pixels(tiles, &rect, p.rows) else { return };
        let i = &p.instance;
        let origin = Vec3::new(i.origin[0], i.origin[1], i.origin[2]);
        let across = Vec3::new(i.axis_x[0], i.axis_x[1], i.axis_x[2]);
        let down = Vec3::new(i.axis_y[0], i.axis_y[1], i.axis_y[2]);
        let step = (rect.cell / FIGURE_PIECES).max(1);
        let middle = origin + across * 0.5 + down * 0.5;
        let size = across.length() * texels_f32(step) / texels_f32(rect.w.max(1));
        for (u, v, colour) in blocks(&pixels, rect.w, p.rows, step, self.lift) {
            let at = origin + across * u + down * v;
            let spread = Vec3::new((self.random)() - 0.5, 0.0, (self.random)() - 0.5) * 2.4;
            let outward = (at - middle) * 2.0;
            let velocity = Vec3::new(outward.x, 1.2 + (self.random)() * 1.8 + outward.y.max(0.0), outward.z) + spread;
            let age = f64::from((self.random)() * 0.3);
            self.pieces.push(Piece { at: at.as_dvec3(), velocity: velocity.as_dvec3(), size: f64::from(size), colour, age });
        }
    }

    pub fn walls_broken(&mut self, old: Option<&Scene>, new: &Scene, tiles: &dyn TileSource) {
        let fresh = std::mem::take(&mut self.fresh_level);
        let Some(old) = old.filter(|_| !fresh) else { return };
        let broken: Vec<(i32, i32, TileId)> = new
            .cells
            .iter()
            .filter(|(_, c)| !c.occluder && c.visibility == Visibility::Visible)
            .filter_map(|(key, c)| {
                let o = old.cells.get(key)?;
                let wall = o.occluder && o.kind == CellKind::Wall && o.visibility != Visibility::Unseen;
                let face = o.face_tiles.and_then(|f| f.n.or(f.s).or(f.w).or(f.e)).or(o.wall_tile)?;
                wall.then_some((c.x, c.y, face))
            })
            .collect();
        if broken.len() > MOST_BROKEN_WALLS {
            return;
        }
        for (x, y, tile) in broken {
            self.break_wall(tiles, x, y, tile);
        }
    }

    fn break_wall(&mut self, tiles: &dyn TileSource, x: i32, y: i32, tile: TileId) {
        let Some(rect) = tiles.tile(tile) else { return };
        let Some(pixels) = tile_pixels(tiles, &rect, rect.h) else { return };
        let colours = blocks(&pixels, rect.w, rect.h, (rect.cell / WALL_PIECES).max(1), self.lift);
        if colours.is_empty() {
            return;
        }
        let size = 1.0 / texels_f32(WALL_PIECES);
        let (cx, cz) = (cell_f32(x), cell_f32(y));
        let mut pick = 0usize;
        for k in 0..WALL_PIECES {
            for j in 0..WALL_PIECES {
                for i in 0..WALL_PIECES {
                    pick = (pick + 7) % colours.len();
                    let colour = colours[pick].2;
                    let at = Vec3::new(cx + (texels_f32(i) + 0.5) * size, (texels_f32(k) + 0.5) * size, cz + (texels_f32(j) + 0.5) * size);
                    let velocity = Vec3::new((self.random)() - 0.5, (self.random)() * 1.5, (self.random)() - 0.5) * 1.6;
                    let (grown, age) = (0.75 + (self.random)() * 0.25, (self.random)() * 0.4);
                    self.pieces.push(Piece { at: at.as_dvec3(), velocity: velocity.as_dvec3(), size: f64::from(size * grown), colour, age: f64::from(age) });
                }
            }
        }
    }

    pub fn update(&mut self, gpu: &Gpu, dt: f64) {
        if self.pieces.is_empty() {
            return;
        }
        for p in &mut self.pieces {
            p.age += dt;
            p.velocity.y -= DEBRIS_GRAVITY * dt;
            p.at += p.velocity * dt;
            let floor = p.size * 0.5;
            if p.at.y < floor {
                p.at.y = floor;
                p.velocity.y = -p.velocity.y * DEBRIS_BOUNCE;
                p.velocity.x *= DEBRIS_FRICTION;
                p.velocity.z *= DEBRIS_FRICTION;
            }
        }
        self.pieces.retain(|p| p.age < DEBRIS_SECONDS);
        let instances: Vec<PieceInstance> = self
            .pieces
            .iter()
            .map(|p| {
                let left = ((DEBRIS_SECONDS - p.age) / DEBRIS_SHRINK_SECONDS).clamp(0.0, 1.0);
                PieceInstance { centre: p.at.extend(p.size * left).as_vec4().to_array(), colour: [p.colour[0], p.colour[1], p.colour[2], 1.0] }
            })
            .collect();
        if self.cube.is_none() {
            self.cube = Some(VoxelGpuMesh::new(&gpu.device, &cube(), 0));
        }
        let buffer = self.piece_instances.get_or_insert_with(|| DynamicBuffer::new(gpu, "voxel pieces", wgpu::BufferUsages::VERTEX, 4096));
        buffer.write(gpu, bytemuck::cast_slice(&instances));
    }

    pub fn prepare(&mut self, gpu: &Gpu, samples: u32, cell_lights: u32, materials: Option<&wgpu::BindGroupLayout>) {
        let key = PipelineKey { samples, cell_lights, materials: materials.is_some() };
        if !self.wanted || self.pipelines.as_ref().is_some_and(|p| p.key == key) {
            return;
        }
        let module = self.modules.entry(key.materials).or_insert_with(|| {
            let source = if key.materials {
                crate::materials::voxel_source()
            } else {
                crate::gpu::with_common(&[include_str!("shaders/voxels.wgsl"), include_str!("shaders/plain_voxels.wgsl")].join("\n"))
            };
            gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("voxels"), source: wgpu::ShaderSource::Wgsl(source.into()) })
        });
        let parts = [VoxelVertex::layout(), VoxelInstance::layout()];
        let pieces = [VoxelVertex::layout(), PieceInstance::layout()];
        let pipeline = |label: &str, vs: &str, fs: &str, buffers: &[wgpu::VertexBufferLayout], groups: &[&wgpu::BindGroupLayout]| {
            layer_pipeline(gpu, label, module, vs, fs, buffers, groups, Blend::Opaque, true, samples, false, Default::default(), cell_lights)
        };
        let paged: Vec<&wgpu::BindGroupLayout> = std::iter::once(&gpu.page_layout).chain(materials).collect();
        self.pipelines = Some(Pipelines {
            key,
            terrain: pipeline("voxel terrain", "vs_voxel", "fs_voxel", &parts, &paged),
            actors: pipeline("voxel actors", "vs_voxel", "fs_voxel_actor", &parts, &paged),
            pieces: pipeline("voxel pieces", "vs_piece", "fs_piece", &pieces, &[]),
        });
    }

    pub fn draw(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        frame: &wgpu::BindGroup,
        materials: Option<&wgpu::BindGroup>,
        chunks: &[&VoxelChunk],
        pages: &Pages,
    ) {
        let mut stats = VoxelStats { meshes: self.meshes.len(), pieces: self.pieces.len(), ..Default::default() };
        let Some(p) = self.pipelines.as_ref().filter(|_| self.wanted) else { return };
        pass.set_pipeline(&p.terrain);
        pass.set_bind_group(0, frame, &[]);
        if let Some(group) = materials.filter(|_| p.key.materials) {
            pass.set_bind_group(2, group, &[]);
        }
        for chunk in chunks {
            draw_parts(pass, &chunk.instances, &chunk.draws, pages, &mut stats);
        }
        if let (Some(buffer), false) = (&self.actor_instances, self.actor_draws.is_empty()) {
            pass.set_pipeline(&p.actors);
            draw_parts(pass, &buffer.buffer, &self.actor_draws, pages, &mut stats);
        }
        if let (Some(buffer), Some(cube), false) = (&self.piece_instances, &self.cube, self.pieces.is_empty()) {
            let count = index_u32(self.pieces.len());
            pass.set_pipeline(&p.pieces);
            pass.set_vertex_buffer(0, cube.vertices.slice(..));
            pass.set_vertex_buffer(1, buffer.buffer.slice(..));
            pass.set_index_buffer(cube.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..cube.count, 0, 0..count);
            stats.draws += 1;
            stats.triangles += cube.triangles() * u64::from(count);
        }
        self.stats = stats;
    }
}

fn draw_parts(pass: &mut wgpu::RenderPass<'_>, buffer: &wgpu::Buffer, draws: &[VoxelDraw], pages: &Pages, stats: &mut VoxelStats) {
    let size = stride_of::<VoxelInstance>();
    let mut page = None;
    for d in draws {
        if page != Some(d.page) {
            let Some(group) = pages.bind_group(d.page) else { continue };
            pass.set_bind_group(1, group, &[]);
            page = Some(d.page);
        }
        pass.set_vertex_buffer(0, d.mesh.vertices.slice(..));
        pass.set_vertex_buffer(1, buffer.slice(u64::from(d.first) * size..u64::from(d.first + d.count) * size));
        pass.set_index_buffer(d.mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..d.mesh.count, 0, 0..d.count);
        stats.draws += 1;
        stats.instances += d.count;
        stats.triangles += d.mesh.triangles() * u64::from(d.count);
    }
}

pub fn layer_depth(depth: f32, index: usize) -> f32 {
    depth * (1.0 + 0.3 * to_f32(index))
}

fn texels_f32(v: u32) -> f32 {
    u16::try_from(v).map(f32::from).unwrap_or(f32::MAX)
}

fn cell_f32(v: i32) -> f32 {
    i16::try_from(v).map(f32::from).unwrap_or(0.0)
}

fn usize_of(v: u32) -> usize {
    usize::try_from(v).unwrap_or(0)
}

fn tile_pixels(tiles: &dyn TileSource, rect: &TileRect, rows: u32) -> Option<Vec<u8>> {
    let atlas = tiles.atlas(&rect.atlas)?;
    if rect.w == 0 || rows == 0 || rect.sx + rect.w > atlas.width || rect.sy + rows > atlas.height {
        return None;
    }
    Some(crate::pages::crop(&atlas, &TileRect { h: rows, ..rect.clone() }))
}

fn blocks(rgba: &[u8], width: u32, rows: u32, step: u32, lift: f32) -> Vec<(f32, f32, [f32; 3])> {
    let mut out = Vec::new();
    let (w, h, s) = (usize_of(width), usize_of(rows), usize_of(step.max(1)));
    for by in (0..h).step_by(s) {
        for bx in (0..w).step_by(s) {
            let (mut sum, mut opaque, mut all) = ([0u32; 3], 0u32, 0u32);
            for ty in by..(by + s).min(h) {
                for tx in bx..(bx + s).min(w) {
                    let i = (ty * w + tx) * 4;
                    let Some(p) = rgba.get(i..i + 4) else { continue };
                    all += 1;
                    if p[3] >= OPAQUE {
                        opaque += 1;
                        for (total, value) in sum.iter_mut().zip(p) {
                            *total += u32::from(*value);
                        }
                    }
                }
            }
            if opaque == 0 || opaque * 2 < all {
                continue;
            }
            let colour = sum.map(|total| painted(texels_f32(total / opaque) / 255.0, lift));
            let middle = |b: usize, size: usize| (to_f32(b) + to_f32(s) * 0.5) / to_f32(size.max(1));
            out.push((middle(bx, w).min(1.0), middle(by, h).min(1.0), colour));
        }
    }
    out
}

fn painted(srgb: f32, lift: f32) -> f32 {
    let c = srgb.clamp(0.0, 1.0).powf(1.0 / lift.max(0.1));
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn cube() -> VoxelMesh {
    let mut m = VoxelMesh::default();
    let (a, b) = (-0.5, 0.5);
    m.quad([[a, b, a], [b, b, a], [b, b, b], [a, b, b]], [0.0, 1.0, 0.0]);
    m.quad([[a, a, a], [a, a, b], [b, a, b], [b, a, a]], [0.0, -1.0, 0.0]);
    m.quad([[b, a, a], [b, a, b], [b, b, b], [b, b, a]], [1.0, 0.0, 0.0]);
    m.quad([[a, a, a], [a, b, a], [a, b, b], [a, a, b]], [-1.0, 0.0, 0.0]);
    m.quad([[a, a, b], [a, b, b], [b, b, b], [b, a, b]], [0.0, 0.0, 1.0]);
    m.quad([[a, a, a], [b, a, a], [b, b, a], [a, b, a]], [0.0, 0.0, -1.0]);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_brick_wall_tile_becomes_bricks_standing_out_of_their_mortar() {
        // given
        let (brick, mortar) = ([200u8, 180, 160, 255], [40u8, 30, 20, 255]);
        let mut pixels = Vec::new();
        for y in 0..16 {
            for _ in 0..16 {
                pixels.extend_from_slice(if (5..8).contains(&y) || y >= 13 { &mortar } else { &brick });
            }
        }

        // when
        let c = columns(&pixels, 16, 16, 1, VoxelKind::Wall);
        let m = mesh(&c);

        // then
        assert_eq!(c.span(0, 1), Some((-WALL_LEVELS, 0)));
        assert_eq!(c.span(0, 6), Some((-WALL_LEVELS, -WALL_LEVELS)));
        let front_heights: Vec<f32> = m.vertices.iter().filter(|v| v.normal == [0.0, 0.0, 1.0]).map(|v| v.position[2]).collect();
        assert!(front_heights.contains(&0.0) && front_heights.contains(&-1.0));
    }
}
