//! Quads in batches (the port of `quads.ts`) and their buffers on the GPU.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

pub type Point3 = [f32; 3];

/// Upright sprites take some light from above: their normals tip up by this much.
pub const SPRITE_LEAN: f32 = 0.3;
/// Atlas texels to keep in from a tile's edges, so filtering does not pick up the next tile.
pub const ATLAS_INSET: f32 = 0.5;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub uv_rect: [f32; 4],
    pub colour: [f32; 3],
    pub normal: [f32; 3],
}

impl Vertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Float32x3, 4 => Float32x3];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &Self::ATTRIBUTES }
    }
}

/// Quads of one atlas page, in texels of the page's source size.
#[derive(Clone, Debug, Default)]
pub struct QuadBatch {
    pub atlas_width: f32,
    pub atlas_height: f32,
    pub lean: f32,
    pub inset: f32,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl QuadBatch {
    pub fn new(atlas_width: f32, atlas_height: f32, lean: f32, inset: f32) -> Self {
        QuadBatch { atlas_width, atlas_height, lean, inset, vertices: Vec::new(), indices: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Corners top-left, top-right, bottom-right, bottom-left of the texel rect; `shade` a colour.
    pub fn add(&mut self, corners: [Point3; 4], sx: f32, sy: f32, w: f32, h: f32, shade: [f32; 3]) {
        let base = self.vertices.len() as u32;
        let ix = self.inset.min(w / 4.0);
        let iy = self.inset.min(h / 4.0);
        let u0 = (sx + ix) / self.atlas_width;
        let u1 = (sx + w - ix) / self.atlas_width;
        let v0 = (sy + iy) / self.atlas_height;
        let v1 = (sy + h - iy) / self.atlas_height;
        let uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        let normal = outward_normal(&corners, self.lean);
        for (c, uv) in corners.iter().zip(uvs) {
            self.vertices.push(Vertex { position: *c, uv, uv_rect: [u0, v0, u1, v1], colour: shade, normal });
        }
        self.indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    pub fn add_grey(&mut self, corners: [Point3; 4], sx: f32, sy: f32, w: f32, h: f32, shade: f32) {
        self.add(corners, sx, sy, w, h, [shade; 3]);
    }

    pub fn append(&mut self, other: &QuadBatch) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }
}

/// The normal on the side the texture faces: down the left edge crossed with along the top edge.
fn outward_normal(c: &[Point3; 4], lean: f32) -> [f32; 3] {
    let down = Vec3::from(c[3]) - Vec3::from(c[0]);
    let along = Vec3::from(c[1]) - Vec3::from(c[0]);
    let mut n = down.cross(along).normalize_or_zero();
    n.y += lean;
    n.normalize_or_zero().to_array()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxSide {
    Top,
    N,
    S,
    W,
    E,
}

pub const ALL_SIDES: [BoxSide; 5] = [BoxSide::Top, BoxSide::N, BoxSide::S, BoxSide::W, BoxSide::E];

/// A box from `min` to `max` (x, up, y) textured with one tile on every face but the bottom; each
/// face takes as much of the tile as it is large, up to the whole tile.
pub fn add_box(batch: &mut QuadBatch, rect: (f32, f32, f32, f32), min: Point3, max: Point3, shade: f32, sides: &[BoxSide]) {
    let (sx, sy, rw, rh) = rect;
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let crop = |width: f32, height: f32| (rw * width.min(1.0), rh * height.min(1.0));
    for &side in sides {
        if side == BoxSide::Top {
            let (w, h) = crop(x1 - x0, z1 - z0);
            batch.add_grey([[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]], sx, sy, w, h, shade);
            continue;
        }
        let (from, to) = match side {
            BoxSide::N => ((1, 0), (0, 0)),
            BoxSide::S => ((0, 1), (1, 1)),
            BoxSide::W => ((0, 0), (0, 1)),
            _ => ((1, 1), (1, 0)),
        };
        let ax = if from.0 == 1 { x1 } else { x0 };
        let az = if from.1 == 1 { z1 } else { z0 };
        let bx = if to.0 == 1 { x1 } else { x0 };
        let bz = if to.1 == 1 { z1 } else { z0 };
        let (w, h) = crop(((bx - ax).powi(2) + (bz - az).powi(2)).sqrt(), y1 - y0);
        batch.add_grey([[ax, y1, az], [bx, y1, bz], [bx, y0, bz], [ax, y0, az]], sx, sy, w, h, shade);
    }
}

/// A batch on the GPU.
pub struct GpuMesh {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub count: u32,
}

impl GpuMesh {
    pub fn new(device: &wgpu::Device, vertices: &[Vertex], indices: &[u32]) -> Self {
        GpuMesh {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("quads"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("quad indices"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            count: indices.len() as u32,
        }
    }

    pub fn from_batch(device: &wgpu::Device, batch: &QuadBatch) -> Self {
        Self::new(device, &batch.vertices, &batch.indices)
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.count, 0, 0..1);
    }
}
