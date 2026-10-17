//! Instanced quads drawn by `particles.wgsl`: one batch per kind, each instance its matrix and fade.

use crate::gpu::{Blend, Gpu};
use crate::layers::{layer_pipeline, shader, uniform_group, uniform_layout, DynamicBuffer};
use crate::lights::linear;
use bytemuck::{Pod, Zeroable};
use glam::Mat4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Instance {
    pub model: [[f32; 4]; 4],
    pub extra: [f32; 4],
}

impl Instance {
    pub fn new(model: Mat4, fade: f32) -> Self {
        Instance { model: model.to_cols_array_2d(), extra: [fade, 0.0, 0.0, 0.0] }
    }
}

const CORNERS: [[f32; 2]; 6] = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];

pub struct ParticleShared {
    pub module: wgpu::ShaderModule,
    pub layout: wgpu::BindGroupLayout,
    pub corners: wgpu::Buffer,
}

impl ParticleShared {
    pub fn new(gpu: &Gpu) -> Self {
        use wgpu::util::DeviceExt;
        ParticleShared {
            module: shader(gpu, "particles", include_str!("shaders/particles.wgsl")),
            layout: uniform_layout(gpu, 16),
            corners: gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("corners"), contents: bytemuck::cast_slice(&CORNERS), usage: wgpu::BufferUsages::VERTEX }),
        }
    }
}

/// A kind of particle: its look (fragment shader, blending), colour and the instances this frame.
pub struct ParticleBatch {
    fs: &'static str,
    blend: Blend,
    depth_write: bool,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    instances: DynamicBuffer,
    count: u32,
    pipeline: Option<wgpu::RenderPipeline>,
}

impl ParticleBatch {
    pub fn new(gpu: &Gpu, shared: &ParticleShared, fs: &'static str, blend: Blend, depth_write: bool) -> Self {
        let uniform = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("particles"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let group = uniform_group(gpu, &shared.layout, &uniform);
        ParticleBatch { fs, blend, depth_write, uniform, group, instances: DynamicBuffer::new(gpu, "particle instances", wgpu::BufferUsages::VERTEX, 4096), count: 0, pipeline: None }
    }

    pub fn prepare(&mut self, gpu: &Gpu, shared: &ParticleShared, samples: u32) {
        let buffers = [
            wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2] },
            wgpu::VertexBufferLayout { array_stride: 80, step_mode: wgpu::VertexStepMode::Instance, attributes: &wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4] },
        ];
        self.pipeline = Some(layer_pipeline(gpu, self.fs, &shared.module, "vs_particle", self.fs, &buffers, &[&shared.layout], self.blend, self.depth_write, samples, self.blend != Blend::Opaque, Default::default()));
    }

    /// The colour, `0xrrggbb` in sRGB, times `k`.
    pub fn colour(&self, gpu: &Gpu, hex: u32, k: f32) {
        let c = linear(hex);
        gpu.queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&[c[0] * k, c[1] * k, c[2] * k, 1.0]));
    }

    pub fn set(&mut self, gpu: &Gpu, instances: &[Instance]) {
        self.instances.write(gpu, bytemuck::cast_slice(instances));
        self.count = instances.len() as u32;
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup, shared: &ParticleShared) {
        let Some(pipeline) = &self.pipeline else { return };
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_vertex_buffer(0, shared.corners.slice(..));
        pass.set_vertex_buffer(1, self.instances.buffer.slice(..));
        pass.draw(0..6, 0..self.count);
    }
}
