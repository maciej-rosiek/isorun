//! The uniforms the shaders read: the frame's (camera, lights, shading) and each draw's.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};

pub const MAX_LIGHTS: usize = 9;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, Default)]
pub struct LightUniform {
    pub position: [f32; 4],
    pub colour: [f32; 4],
    pub shadow: [f32; 4],
    pub world: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct FrameUniform {
    pub view_proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub inv_view: [[f32; 4]; 4],
    pub sun_matrix: [[f32; 4]; 4],
    pub camera: [f32; 4],
    pub sky: [f32; 4],
    pub ground: [f32; 4],
    pub sun_dir: [f32; 4],
    pub sun_colour: [f32; 4],
    pub sun_shadow: [f32; 4],
    pub shading0: [f32; 4],
    pub shading1: [f32; 4],
    pub shading2: [f32; 4],
    pub shading3: [f32; 4],
    pub viewport: [f32; 4],
    pub up: [f32; 4],
    pub lights: [LightUniform; MAX_LIGHTS],
}

impl Default for FrameUniform {
    fn default() -> Self {
        Zeroable::zeroed()
    }
}

/// What a draw adds to the frame: where its mesh is, and how an actor looks now.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DrawUniform {
    pub model: [[f32; 4]; 4],
    /// rgb tint, w opacity
    pub tint: [f32; 4],
    /// motion, phase, receives shadows, takes the memory shade
    pub look: [f32; 4],
    /// petrified, burning, poisoned, corroded
    pub harm: [f32; 4],
    /// berserk, glowing, summoned, wet
    pub aura: [f32; 4],
}

impl DrawUniform {
    pub fn plain(receive_shadow: bool) -> Self {
        DrawUniform { model: Mat4::IDENTITY.to_cols_array_2d(), tint: [1.0; 4], look: [0.0, 0.0, receive_shadow as u8 as f32, 1.0], harm: [0.0; 4], aura: [0.0; 4] }
    }

    pub fn at(model: Mat4, receive_shadow: bool) -> Self {
        DrawUniform { model: model.to_cols_array_2d(), ..Self::plain(receive_shadow) }
    }
}

/// The per-draw uniforms of a frame in one buffer, each at a 256-byte offset.
pub struct DrawBuffer {
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    capacity: usize,
    data: Vec<u8>,
}

pub const DRAW_STRIDE: usize = 256;

impl DrawBuffer {
    pub fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, capacity: usize) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("draws"), size: (capacity * DRAW_STRIDE) as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let bind_group = draw_bind_group(device, layout, &buffer);
        DrawBuffer { buffer, bind_group, capacity, data: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Adds a draw's uniforms; returns the dynamic offset to bind it at.
    pub fn push(&mut self, draw: &DrawUniform) -> u32 {
        let offset = self.data.len();
        self.data.extend_from_slice(bytemuck::bytes_of(draw));
        self.data.resize(offset + DRAW_STRIDE, 0);
        offset as u32
    }

    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, layout: &wgpu::BindGroupLayout) {
        let needed = self.data.len() / DRAW_STRIDE;
        if needed > self.capacity {
            let data = self.take_data();
            *self = DrawBuffer::new(device, layout, needed.next_power_of_two());
            self.data = data;
        }
        if !self.data.is_empty() {
            queue.write_buffer(&self.buffer, 0, &self.data);
        }
    }

    pub fn take_data(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.data)
    }

    pub fn set_data(&mut self, data: Vec<u8>) {
        self.data = data;
    }
}

pub fn draw_bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, buffer: &wgpu::Buffer) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("draw"),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer, offset: 0, size: std::num::NonZeroU64::new(std::mem::size_of::<DrawUniform>() as u64) }) }],
    })
}

pub fn v4(v: Vec4) -> [f32; 4] {
    v.to_array()
}
