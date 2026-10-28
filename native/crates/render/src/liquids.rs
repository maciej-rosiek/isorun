//! The liquids (the port of `water.ts`, `lava.ts`, `starwater.ts`): one fine grid per liquid over
//! its seen cells and the land around them, drawn blended over the floor laid under it.

use crate::gpu::{Blend, Gpu};
use crate::layers::{layer_pipeline, shader, uniform_group, uniform_layout, DynamicBuffer};
use crate::lights::{linear, IsoLights};
use bytemuck::{Pod, Zeroable};
use isorun_scene::{LiquidKind, Scene, Visibility};
use std::collections::BTreeMap;

const SURFACE_HEIGHT: f32 = 0.02;
const SUBDIVISIONS: usize = 4;
const SHORE_ROUNDING: f32 = 0.45;
const NEAR_ROUNDING: f32 = 0.9;
const DEEP_DISTANCE: f32 = 1.0;
const SHALLOW_DEPTH: f32 = 0.25;
const WATER_GLOSS: f32 = 0.45;

/// The liquids laid on the ground as films, over their own floor tile.
pub const FILMS: [LiquidKind; 2] = [LiquidKind::Slime, LiquidKind::Acid];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GridVertex {
    position: [f32; 3],
    liquid: f32,
    near: f32,
    depth: f32,
}

/// A fine grid over the seen cells of one liquid and the land around them.
struct LiquidGrid {
    kind: LiquidKind,
    vertices: DynamicBuffer,
    indices: DynamicBuffer,
    count: u32,
    built_key: u64,
}

impl LiquidGrid {
    fn new(gpu: &Gpu, kind: LiquidKind) -> Self {
        LiquidGrid {
            kind,
            vertices: DynamicBuffer::new(gpu, "liquid", wgpu::BufferUsages::VERTEX, 1024),
            indices: DynamicBuffer::new(gpu, "liquid indices", wgpu::BufferUsages::INDEX, 1024),
            count: 0,
            built_key: 0,
        }
    }

    fn build(&mut self, gpu: &Gpu, scene: &Scene) {
        use std::hash::{Hash, Hasher};
        let holds = |x: i32, y: i32| scene.cell(x, y).and_then(|c| c.liquid).is_some_and(|l| l.kind == self.kind);
        let mut pools: Vec<_> = scene.cells.values().filter(|c| c.liquid.is_some_and(|l| l.kind == self.kind) && c.visibility != Visibility::Unseen).collect();
        pools.sort_by_key(|c| (c.y, c.x));
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for c in &pools {
            (c.x, c.y, c.liquid.map(|l| l.deep)).hash(&mut h);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let n = scene.cell(c.x + dx, c.y + dy);
                    (n.map(|n| n.visibility == Visibility::Unseen), n.map(|n| n.occluder), n.and_then(|n| n.liquid.map(|l| l.kind))).hash(&mut h);
                }
            }
        }
        let key = h.finish() | 1;
        if key == self.built_key {
            return;
        }
        self.built_key = key;
        if pools.is_empty() {
            self.count = 0;
            return;
        }
        let x0 = pools.iter().map(|c| c.x).min().unwrap() - 2;
        let y0 = pools.iter().map(|c| c.y).min().unwrap() - 2;
        let w = (pools.iter().map(|c| c.x).max().unwrap() + 2 - x0 + 1) as usize;
        let hh = (pools.iter().map(|c| c.y).max().unwrap() + 2 - y0 + 1) as usize;
        let mask: Vec<f32> = (0..w * hh).map(|i| holds(x0 + (i % w) as i32, y0 + (i / w) as i32) as u8 as f32).collect();
        let field = crate::fog::blur(&mask, w, hh, SHORE_ROUNDING);
        let near_field = crate::fog::blur(&mask, w, hh, NEAR_ROUNDING);
        let mut distance: Vec<f32> = mask.iter().map(|&m| if m > 0.0 { f32::INFINITY } else { 0.0 }).collect();
        let none = vec![0u8; w * hh];
        crate::fog::chamfer(&mut distance, &none, w, hh);
        crate::fog::chamfer(&mut distance, &none, w, hh);
        let deepest: Vec<f32> = (0..w * hh)
            .map(|i| if scene.cell(x0 + (i % w) as i32, y0 + (i / w) as i32).and_then(|c| c.liquid).is_some_and(|l| !l.deep) { SHALLOW_DEPTH } else { 1.0 })
            .collect();
        let depth_field: Vec<f32> = distance.iter().zip(&deepest).map(|(d, m)| m.min(((d - 0.5) / DEEP_DISTANCE).max(0.0))).collect();
        let sample = |grid: &[f32], px: f32, py: f32| {
            let fx = px - x0 as f32 - 0.5;
            let fy = py - y0 as f32 - 0.5;
            let (ix, iy) = (fx.floor() as i32, fy.floor() as i32);
            let at = |x: i32, y: i32| grid[(y.clamp(0, hh as i32 - 1) as usize) * w + x.clamp(0, w as i32 - 1) as usize];
            let (tx, ty) = (fx - ix as f32, fy - iy as f32);
            let top = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * tx;
            let bottom = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * tx;
            top + (bottom - top) * ty
        };
        let mut covered: BTreeMap<(i32, i32), ()> = BTreeMap::new();
        for c in &pools {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(n) = scene.cell(c.x + dx, c.y + dy) {
                        if !n.occluder && n.visibility != Visibility::Unseen {
                            covered.insert((n.y, n.x), ());
                        }
                    }
                }
            }
        }
        let row = SUBDIVISIONS + 1;
        let mut vertices = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        for &(y, x) in covered.keys() {
            let base = vertices.len() as u32;
            for j in 0..=SUBDIVISIONS {
                for i in 0..=SUBDIVISIONS {
                    let px = x as f32 + i as f32 / SUBDIVISIONS as f32;
                    let py = y as f32 + j as f32 / SUBDIVISIONS as f32;
                    vertices.push(GridVertex {
                        position: [px, SURFACE_HEIGHT, py],
                        liquid: sample(&field, px, py),
                        near: sample(&near_field, px, py),
                        depth: sample(&depth_field, px, py),
                    });
                }
            }
            for j in 0..SUBDIVISIONS {
                for i in 0..SUBDIVISIONS {
                    let a = base + (j * row + i) as u32;
                    let r = row as u32;
                    indices.extend([a, a + r + 1, a + 1, a, a + r, a + r + 1]);
                }
            }
        }
        self.vertices.write(gpu, bytemuck::cast_slice(&vertices));
        self.indices.write(gpu, bytemuck::cast_slice(&indices));
        self.count = indices.len() as u32;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LiquidUniform {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
    params: [f32; 4],
}

struct Pool {
    grid: LiquidGrid,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    shader: &'static str,
    shown: bool,
}

pub struct Liquids {
    pools: Vec<Pool>,
    layout: wgpu::BindGroupLayout,
    module: wgpu::ShaderModule,
    pipelines: Vec<(&'static str, wgpu::RenderPipeline)>,
    built_revision: u64,
}

/// The palettes of the waters that are not clear water: deep, shallow, light, strength, speed, opacity, gloss.
fn palette(kind: LiquidKind) -> (u32, u32, u32, f32, f32, f32, f32) {
    match kind {
        LiquidKind::Murky => (0x18200f, 0x0c1207, 0x6f7c3c, 1.0, 0.6, 1.0, 0.3),
        LiquidKind::Bog => (0x1d360c, 0x284612, 0x8cc63c, 2.0, 0.35, 1.2, 0.25),
        LiquidKind::Mud => (0x1e140c, 0x281b10, 0x7a6044, 1.0, 0.12, 1.6, 0.15),
        LiquidKind::Slime => (0x15280a, 0x1c340c, 0xb0f060, 2.5, 0.2, 0.4, 0.5),
        _ => (0x3c400a, 0x4a4e0c, 0xe6f070, 2.5, 0.45, 0.8, 0.4),
    }
}

impl Liquids {
    /// Some liquid is drawn at all (its ripples, stars and lava move).
    pub fn drawn(&self) -> bool {
        self.pools.iter().any(|p| p.shown && p.grid.count > 0)
    }

    pub fn new(gpu: &Gpu) -> Self {
        let layout = uniform_layout(gpu, 64);
        let module = shader(gpu, "liquids", include_str!("shaders/liquids.wgsl"));
        let kinds = [
            (LiquidKind::Water, "fs_water"),
            (LiquidKind::Murky, "fs_water"),
            (LiquidKind::Bog, "fs_water"),
            (LiquidKind::Mud, "fs_water"),
            (LiquidKind::Slime, "fs_water"),
            (LiquidKind::Acid, "fs_water"),
            (LiquidKind::Starwater, "fs_starwater"),
            (LiquidKind::Lava, "fs_lava"),
        ];
        let pools = kinds
            .iter()
            .map(|&(kind, shader)| {
                let uniform = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("liquid"),
                    size: 64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let group = uniform_group(gpu, &layout, &uniform);
                Pool { grid: LiquidGrid::new(gpu, kind), uniform, group, shader, shown: true }
            })
            .collect();
        Liquids { pools, layout, module, pipelines: Vec::new(), built_revision: u64::MAX }
    }

    pub fn prepare(&mut self, gpu: &Gpu, samples: u32, sight_light: bool) {
        let buffers = [wgpu::VertexBufferLayout {
            array_stride: 24,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32, 3 => Float32],
        }];
        self.pipelines = ["fs_water", "fs_lava", "fs_starwater"]
            .iter()
            .map(|fs| {
                let pipeline = layer_pipeline(
                    gpu,
                    fs,
                    &self.module,
                    "vs_liquid",
                    fs,
                    &buffers,
                    &[&self.layout],
                    Blend::Alpha,
                    false,
                    samples,
                    true,
                    Default::default(),
                    sight_light,
                );
                (*fs, pipeline)
            })
            .collect();
    }

    /// The colours and the speeds of the look; the water's own, the other waters' from their palettes.
    pub fn configure(&mut self, gpu: &Gpu, lights: &IsoLights) {
        let lin = |h| {
            let l = linear(h);
            [l[0], l[1], l[2], 1.0]
        };
        for pool in &mut self.pools {
            let kind = pool.grid.kind;
            let u = match kind {
                LiquidKind::Lava => {
                    pool.shown = lights.lava.enabled;
                    LiquidUniform {
                        a: lin(lights.lava.crust),
                        b: lin(lights.lava.molten),
                        c: lin(lights.lava.hot),
                        params: [lights.lava.glow, lights.lava.speed, 0.0, 0.0],
                    }
                }
                LiquidKind::Starwater => {
                    pool.shown = lights.water.enabled;
                    LiquidUniform { a: [0.0; 4], b: [0.0; 4], c: [0.0; 4], params: [0.0; 4] }
                }
                LiquidKind::Water => {
                    pool.shown = lights.water.enabled;
                    let w = &lights.water;
                    LiquidUniform { a: lin(w.deep), b: lin(w.shallow), c: lin(w.light), params: [w.strength.min(1.0), w.speed, 1.0, WATER_GLOSS] }
                }
                _ => {
                    pool.shown = lights.water.enabled;
                    let (deep, shallow, light, strength, speed, opacity, gloss) = palette(kind);
                    LiquidUniform {
                        a: lin(deep),
                        b: lin(shallow),
                        c: lin(light),
                        params: [(lights.water.strength * strength).min(1.0), lights.water.speed * speed, opacity, gloss],
                    }
                }
            };
            gpu.queue.write_buffer(&pool.uniform, 0, bytemuck::bytes_of(&u));
        }
    }

    pub fn build(&mut self, gpu: &Gpu, scene: &Scene, revision: u64) {
        if revision == self.built_revision {
            return;
        }
        self.built_revision = revision;
        for pool in &mut self.pools {
            pool.grid.build(gpu, scene);
        }
    }

    /// Whether lava or starwater is drawn: the screen blooms what they give.
    pub fn glowing(&self) -> bool {
        self.pools.iter().any(|p| matches!(p.grid.kind, LiquidKind::Lava | LiquidKind::Starwater) && p.shown && p.grid.count > 0)
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        for pool in &self.pools {
            if !pool.shown || pool.grid.count == 0 {
                continue;
            }
            let Some((_, pipeline)) = self.pipelines.iter().find(|(n, _)| *n == pool.shader) else { continue };
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, frame, &[]);
            pass.set_bind_group(1, &pool.group, &[]);
            pass.set_vertex_buffer(0, pool.grid.vertices.buffer.slice(..));
            pass.set_index_buffer(pool.grid.indices.buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..pool.grid.count, 0, 0..1);
        }
    }
}
