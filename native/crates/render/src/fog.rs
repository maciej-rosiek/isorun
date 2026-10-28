//! A drifting fog over the unexplored cells the explored floor opens onto (the port of `fog.ts`).
//! A change of the explored area blends the fog from where it is to the new shape.

use crate::gpu::{Blend, Gpu};
use crate::layers::{layer_pipeline, shader, DynamicBuffer};
use crate::lights::{linear, FogSettings};
use bytemuck::{Pod, Zeroable};
use isorun_scene::{Scene, SceneCell, Visibility};

const NOISE_SIZE: usize = 128;
const NOISE_OCTAVES: [usize; 3] = [8, 16, 32];
const LAYERS: u32 = 10;
const SHAPE_BLUR: f32 = 0.6;
const SHAPE_GAIN: f32 = 2.0;
const VISIBLE_FOG: f32 = 0.002;
const CHAMFER_ROUNDS: usize = 3;
const FADE_SECONDS: f32 = 0.8;

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// three's MathUtils.smoothstep(x, min, max).
fn smooth(x: f32, min: f32, max: f32) -> f32 {
    if x <= min {
        return 0.0;
    }
    if x >= max {
        return 1.0;
    }
    let t = (x - min) / (max - min);
    t * t * (3.0 - 2.0 * t)
}

/// Distance in cells to the nearest zero around blocked cells, in place (a chamfer pass each way).
pub fn chamfer(d: &mut [f32], blocked: &[u8], w: usize, h: usize) {
    let diag = std::f32::consts::SQRT_2;
    let relax = |d: &mut [f32], i: usize, j: usize, step: f32| {
        if blocked[i] == 0 && blocked[j] == 0 {
            d[i] = d[i].min(d[j] + step);
        }
    };
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x > 0 {
                relax(d, i, i - 1, 1.0)
            }
            if y > 0 {
                relax(d, i, i - w, 1.0)
            }
            if x > 0 && y > 0 {
                relax(d, i, i - w - 1, diag)
            }
            if x < w - 1 && y > 0 {
                relax(d, i, i - w + 1, diag)
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if x < w - 1 {
                relax(d, i, i + 1, 1.0)
            }
            if y < h - 1 {
                relax(d, i, i + w, 1.0)
            }
            if x < w - 1 && y < h - 1 {
                relax(d, i, i + w + 1, diag)
            }
            if x > 0 && y < h - 1 {
                relax(d, i, i + w - 1, diag)
            }
        }
    }
}

/// A Gaussian blur of a grid of values, `sigma` cells wide; 0 leaves it as it is.
pub fn blur(values: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    if sigma <= 0.0 {
        return values.to_vec();
    }
    let radius = (sigma * 2.0).ceil() as isize;
    let kernel: Vec<f32> = (-radius..=radius).map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp()).collect();
    let total: f32 = kernel.iter().sum();
    let pass = |src: &[f32], dx: isize, dy: isize| {
        let mut out = vec![0f32; src.len()];
        for y in 0..h as isize {
            for x in 0..w as isize {
                let mut sum = 0.0;
                for k in -radius..=radius {
                    let (sx, sy) = (x + k * dx, y + k * dy);
                    if sx >= 0 && sy >= 0 && sx < w as isize && sy < h as isize {
                        sum += src[(sy as usize) * w + sx as usize] * kernel[(k + radius) as usize];
                    }
                }
                out[(y as usize) * w + x as usize] = sum / total;
            }
        }
        out
    };
    pass(&pass(values, 1, 0), 0, 1)
}

/// Tileable value noise, several octaves, 0..255 in every channel.
pub fn noise_pixels() -> Vec<u8> {
    let mut field = vec![0f32; NOISE_SIZE * NOISE_SIZE];
    let mut seed: u32 = 7;
    let mut random = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        seed as f32 / 4294967296.0
    };
    let mut total = 0.0;
    for (octave, &cells) in NOISE_OCTAVES.iter().enumerate() {
        let weight = 1.0 / (octave as f32 + 1.0);
        total += weight;
        let grid: Vec<f32> = (0..cells * cells).map(|_| random()).collect();
        let at = |gx: usize, gy: usize| grid[(gy % cells) * cells + gx % cells];
        for y in 0..NOISE_SIZE {
            for x in 0..NOISE_SIZE {
                let fx = x as f32 / NOISE_SIZE as f32 * cells as f32;
                let fy = y as f32 / NOISE_SIZE as f32 * cells as f32;
                let (gx, gy) = (fx.floor() as usize, fy.floor() as usize);
                let tx = smooth(fx - gx as f32, 0.0, 1.0);
                let ty = smooth(fy - gy as f32, 0.0, 1.0);
                let top = at(gx, gy) + (at(gx + 1, gy) - at(gx, gy)) * tx;
                let bottom = at(gx, gy + 1) + (at(gx + 1, gy + 1) - at(gx, gy + 1)) * tx;
                field[y * NOISE_SIZE + x] += (top + (bottom - top) * ty) * weight;
            }
        }
    }
    field
        .iter()
        .flat_map(|v| {
            let b = ((v / total) * 255.0).round() as u8;
            [b, b, b, 255]
        })
        .collect()
}

pub fn noise_texture(gpu: &Gpu) -> wgpu::Texture {
    gpu.texture_with("noise", NOISE_SIZE as u32, NOISE_SIZE as u32, wgpu::TextureFormat::Rgba8Unorm, &noise_pixels())
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FogUniform {
    colour: [f32; 4],
    params: [f32; 4],
    grid: [f32; 4],
}

struct Shape {
    field: Vec<u8>,
    mask: Vec<u8>,
}

/// How thick the fog is over each cell of a grid, and which cells are explored.
fn fog_shape(explored: &[&SceneCell], reach: f32, x0: i32, y0: i32, w: usize, h: usize) -> Shape {
    let mut distance = vec![f32::INFINITY; w * h];
    let mut blocked = vec![0u8; w * h];
    for c in explored {
        let i = (c.y - y0) as usize * w + (c.x - x0) as usize;
        if c.occluder {
            blocked[i] = 1;
        } else {
            distance[i] = 0.0;
        }
    }
    let mask: Vec<u8> = (0..w * h).map(|i| if distance[i] == 0.0 || blocked[i] == 1 { 255 } else { 0 }).collect();
    for _ in 0..CHAMFER_ROUNDS {
        chamfer(&mut distance, &blocked, w, h);
    }
    let full = reach / 2.0;
    let thickness: Vec<f32> = (0..w * h)
        .map(|i| {
            let d = distance[i];
            if reach <= 0.0 || blocked[i] == 1 || d == 0.0 || !d.is_finite() {
                return 0.0;
            }
            smooth(d, 0.0, full) * (1.0 - smooth(d, reach - full, reach))
        })
        .collect();
    let blurred = blur(&thickness, w, h, SHAPE_BLUR);
    let field = blurred.iter().map(|&v| if v < VISIBLE_FOG { 0 } else { ((v * SHAPE_GAIN).min(1.0) * 255.0).round() as u8 }).collect();
    Shape { field, mask }
}

pub struct FogLayer {
    enabled: bool,
    settings: FogSettings,
    layout: wgpu::BindGroupLayout,
    module: wgpu::ShaderModule,
    pipeline: Option<wgpu::RenderPipeline>,
    uniform: wgpu::Buffer,
    group: Option<wgpu::BindGroup>,
    textures: Option<[wgpu::Texture; 4]>,
    vertices: DynamicBuffer,
    indices: DynamicBuffer,
    index_count: u32,
    grid: (i32, i32, usize, usize),
    from: Shape,
    to: Shape,
    shape_key: (usize, i32),
    height: f32,
    reach: f32,
    built_revision: u64,
    blend: f32,
    first: bool,
}

impl FogLayer {
    /// A new explored area is blending in (`FADE_SECONDS`).
    pub fn fading(&self) -> bool {
        self.blend < 1.0
    }

    pub fn new(gpu: &Gpu) -> Self {
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: float, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fog"),
            entries: &[crate::gpu::uniform_entry(0, 48, false), tex(1), tex(2), tex(3), tex(4)],
        });
        let uniform = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fog"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        FogLayer {
            enabled: true,
            settings: crate::lights::diorama().fog,
            layout,
            module: shader(gpu, "fog", include_str!("shaders/fog.wgsl")),
            pipeline: None,
            uniform,
            group: None,
            textures: None,
            vertices: DynamicBuffer::new(gpu, "fog", wgpu::BufferUsages::VERTEX, 1024),
            indices: DynamicBuffer::new(gpu, "fog indices", wgpu::BufferUsages::INDEX, 1024),
            index_count: 0,
            grid: (0, 0, 0, 0),
            from: Shape { field: Vec::new(), mask: Vec::new() },
            to: Shape { field: Vec::new(), mask: Vec::new() },
            shape_key: (usize::MAX, 0),
            height: 0.0,
            reach: 0.0,
            built_revision: u64::MAX,
            blend: 1.0,
            first: true,
        }
    }

    pub fn prepare(&mut self, gpu: &Gpu, samples: u32) {
        let buffers =
            [wgpu::VertexBufferLayout { array_stride: 12, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x3] }];
        self.pipeline = Some(layer_pipeline(
            gpu,
            "fog",
            &self.module,
            "vs_fog",
            "fs_fog",
            &buffers,
            &[&self.layout],
            Blend::Alpha,
            false,
            samples,
            true,
            Default::default(),
            0,
        ));
    }

    pub fn configure(&mut self, settings: &FogSettings) {
        self.settings = *settings;
        self.enabled = settings.enabled;
    }

    pub fn update(&mut self, dt: f32) {
        self.blend = (self.blend + dt / FADE_SECONDS).min(1.0);
    }

    /// The fog over the unexplored cells the explored floor opens onto; the same explored area keeps the fog as it is.
    pub fn build(&mut self, gpu: &Gpu, scene: &Scene, revision: u64, height: f32, reach: f32) {
        if revision == self.built_revision && height == self.height && reach == self.reach {
            return;
        }
        self.built_revision = revision;
        self.reach = reach;
        let mut explored = Vec::new();
        let mut shape: i32 = reach as i32;
        let (mut left, mut top, mut right, mut bottom) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        let mut cells: Vec<&SceneCell> = scene.cells.values().collect();
        cells.sort_by_key(|c| (c.y, c.x));
        for c in cells {
            left = left.min(c.x);
            top = top.min(c.y);
            right = right.max(c.x);
            bottom = bottom.max(c.y);
            if c.visibility == Visibility::Unseen {
                continue;
            }
            explored.push(c);
            shape = shape.wrapping_mul(31).wrapping_add(c.x.wrapping_mul(1031)).wrapping_add(c.y * 7).wrapping_add(c.occluder as i32);
        }
        let shape_key = (explored.len(), shape);
        if shape_key == self.shape_key && height == self.height {
            return;
        }
        if scene.cells.is_empty() {
            return;
        }
        let margin = (reach + 2.0 * SHAPE_BLUR).ceil() as i32 + 1;
        let (x0, y0) = (left - margin, top - margin);
        let (w, h) = ((right + margin - x0 + 1) as usize, (bottom + margin - y0 + 1) as usize);
        let next = fog_shape(&explored, reach, x0, y0, w, h);
        if shape_key != self.shape_key {
            let t = smooth(self.blend, 0.0, 1.0);
            let old = self.grid;
            let shown = |a: &[u8], b: &[u8], x: usize, y: usize, fallback: u8| -> u8 {
                let ox = x as i32 + x0 - old.0;
                let oy = y as i32 + y0 - old.1;
                if ox < 0 || oy < 0 || ox as usize >= old.2 || oy as usize >= old.3 || a.len() != old.2 * old.3 {
                    return fallback;
                }
                let i = oy as usize * old.2 + ox as usize;
                (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8
            };
            let mut field = vec![0u8; w * h];
            let mut mask = vec![0u8; w * h];
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    field[i] = shown(&self.from.field, &self.to.field, x, y, next.field[i]);
                    mask[i] = shown(&self.from.mask, &self.to.mask, x, y, next.mask[i]);
                }
            }
            self.from = Shape { field, mask };
            self.to = next;
            self.blend = if self.first { 1.0 } else { 0.0 };
            self.first = false;
        }
        self.shape_key = shape_key;
        self.height = height;
        self.grid = (x0, y0, w, h);
        let rgba = |v: &[u8]| v.iter().flat_map(|&b| [b, 0, 0, 255]).collect::<Vec<u8>>();
        let textures = [&self.from.field, &self.to.field, &self.from.mask, &self.to.mask]
            .map(|d| gpu.texture_with("fog", w as u32, h as u32, wgpu::TextureFormat::Rgba8Unorm, &rgba(d)));
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        self.group = Some(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fog"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view(&textures[0])) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&view(&textures[1])) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&view(&textures[2])) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&view(&textures[3])) },
            ],
        }));
        self.textures = Some(textures);
        // one quad a cell wherever there is fog or fog next to it
        let foggy: Vec<bool> = (0..w * h).map(|i| self.from.field[i] > 0 || self.to.field[i] > 0).collect();
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let near = (-1i32..=1).any(|dy| {
                    (-1i32..=1).any(|dx| {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h && foggy[ny as usize * w + nx as usize]
                    })
                });
                if !near {
                    continue;
                }
                let (cx, cz) = ((x as i32 + x0) as f32, (y as i32 + y0) as f32);
                let base = positions.len() as u32;
                positions.extend([[cx, 0.0, cz], [cx + 1.0, 0.0, cz], [cx + 1.0, 0.0, cz + 1.0], [cx, 0.0, cz + 1.0]]);
                indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
            }
        }
        self.vertices.write(gpu, bytemuck::cast_slice(&positions));
        self.indices.write(gpu, bytemuck::cast_slice(&indices));
        self.index_count = indices.len() as u32;
    }

    /// The fog's uniforms for this frame, before the pass that draws it.
    pub fn upload(&self, gpu: &Gpu) {
        let c = linear(self.settings.colour);
        let (x0, y0, w, h) = self.grid;
        let u = FogUniform {
            colour: [c[0], c[1], c[2], 1.0],
            params: [self.settings.density, self.settings.softness, self.height, self.blend],
            grid: [x0 as f32, y0 as f32, w as f32, h as f32],
        };
        gpu.queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        if !self.enabled || self.index_count == 0 {
            return;
        }
        let (Some(pipeline), Some(group)) = (&self.pipeline, &self.group) else { return };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, group, &[]);
        pass.set_vertex_buffer(0, self.vertices.buffer.slice(..));
        pass.set_index_buffer(self.indices.buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..LAYERS);
    }
}

#[allow(dead_code)]
fn unused() {
    let _ = smoothstep;
}
