//! The shadow maps: the sun's (one orthographic depth map that follows the camera a whole cell at
//! a time) and the torches' (six views each, the distance to the torch), drawn only when stale.

use crate::gpu::{Gpu, DEPTH_FORMAT};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

pub const SHADOW_MAP_SIZE: u32 = 512;
pub const SUN_SHADOW_MAP_SIZE: u32 = 2048;
pub const TORCH_NEAR: f32 = 0.05;
pub const SUN_REACH: f32 = 18.0;
pub const SUN_DISTANCE: f32 = 30.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ShadowViewUniform {
    pub view_proj: [[f32; 4]; 4],
    pub light: [f32; 4],
    pub mode: [f32; 4],
}

const VIEW_STRIDE: u64 = 256;

/// The six views of a torch's shadow: rows that turn a direction from the torch into the camera's
/// x, y and z, laid out as `cube_face` in common.wgsl reads them back.
const FACES: [[[f32; 3]; 3]; 6] = [
    [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]],
    [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
    [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, -1.0, 0.0]],
    [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]],
    [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
];

/// The view and projection of one face of a torch's shadow.
pub fn cube_face_view(face: usize, at: Vec3, far: f32) -> Mat4 {
    let r = FACES[face];
    let rot = Mat4::from_cols(
        Vec4::new(r[0][0], r[1][0], r[2][0], 0.0),
        Vec4::new(r[0][1], r[1][1], r[2][1], 0.0),
        Vec4::new(r[0][2], r[1][2], r[2][2], 0.0),
        Vec4::W,
    );
    let view = rot * Mat4::from_translation(-at);
    Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, TORCH_NEAR, far) * view
}

pub struct Shadows {
    pub sun: wgpu::Texture,
    pub sun_view: wgpu::TextureView,
    pub points: wgpu::Texture,
    pub points_view: wgpu::TextureView,
    pub point_lights: u32,
    pub views: wgpu::Buffer,
    pub view_group: wgpu::BindGroup,
    scratch: Vec<u8>,
}

impl Shadows {
    pub fn new(gpu: &Gpu, point_lights: u32) -> Self {
        let device = &gpu.device;
        let sun = gpu.texture(
            "sun shadow",
            SUN_SHADOW_MAP_SIZE,
            SUN_SHADOW_MAP_SIZE,
            DEPTH_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        // OpenGL picks a texture's kind from its shape: a square one of 6n layers is a cube map
        // (array), which WebGL2 cannot draw into by layer or has not at all, and one of a single
        // layer is no array; a spare layer keeps it a 2D array there
        let layers = if gpu.gl { (point_lights * 6 + 1).max(2) } else { (point_lights * 6).max(1) };
        let points = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("torch shadows"),
            size: wgpu::Extent3d { width: SHADOW_MAP_SIZE, height: SHADOW_MAP_SIZE, depth_or_array_layers: layers },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let points_view = points.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let views = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow views"),
            size: VIEW_STRIDE * (1 + layers as u64),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow view"),
            layout: &gpu.shadow_view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &views, offset: 0, size: std::num::NonZeroU64::new(96) }),
            }],
        });
        let sun_view = sun.create_view(&wgpu::TextureViewDescriptor::default());
        Shadows { sun, sun_view, points, points_view, point_lights, views, view_group, scratch: Vec::new() }
    }

    pub fn begin(&mut self) {
        self.scratch.clear();
    }

    /// Adds a view to draw a shadow from; returns its offset.
    pub fn view(&mut self, view_proj: Mat4, light: Vec4, point: bool) -> u32 {
        let offset = self.scratch.len() as u32;
        self.scratch.extend_from_slice(bytemuck::bytes_of(&ShadowViewUniform {
            view_proj: view_proj.to_cols_array_2d(),
            light: light.to_array(),
            mode: [point as u8 as f32, 0.0, 0.0, 0.0],
        }));
        self.scratch.resize(offset as usize + VIEW_STRIDE as usize, 0);
        offset
    }

    pub fn upload(&self, queue: &wgpu::Queue) {
        if !self.scratch.is_empty() {
            queue.write_buffer(&self.views, 0, &self.scratch);
        }
    }

    pub fn layer_view(&self, layer: u32) -> wgpu::TextureView {
        self.points.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    }
}
