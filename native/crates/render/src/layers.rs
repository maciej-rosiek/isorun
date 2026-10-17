//! Pipelines for the layers drawn over the level's materials: blended and additive meshes (fog,
//! liquids, blood, dust, glows) and the pointer. They read the frame's bindings (common.wgsl).

use crate::gpu::{scene_targets, Blend, Gpu, DEPTH_FORMAT};

/// A pipeline over the frame's bind group and up to two of the layer's own.
pub fn layer_pipeline(
    gpu: &Gpu,
    label: &str,
    module: &wgpu::ShaderModule,
    vs: &str,
    fs: &str,
    buffers: &[wgpu::VertexBufferLayout],
    groups: &[&wgpu::BindGroupLayout],
    blend: Blend,
    depth_write: bool,
    samples: u32,
    keep_normal: bool,
    bias: wgpu::DepthBiasState,
) -> wgpu::RenderPipeline {
    let mut layouts: Vec<Option<&wgpu::BindGroupLayout>> = vec![Some(&gpu.frame_layout)];
    layouts.extend(groups.iter().map(|g| Some(*g)));
    let layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &layouts, immediate_size: 0 });
    let targets = scene_targets(blend, keep_normal);
    let buffers: Vec<Option<wgpu::VertexBufferLayout>> = buffers.iter().cloned().map(Some).collect();
    gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module, entry_point: Some(vs), buffers: &buffers, compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module, entry_point: Some(fs), targets: &targets, compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH_FORMAT, depth_write_enabled: Some(depth_write), depth_compare: Some(wgpu::CompareFunction::LessEqual), stencil: Default::default(), bias }),
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        multiview_mask: None,
        cache: None,
    })
}

pub fn shader(gpu: &Gpu, label: &str, source: &str) -> wgpu::ShaderModule {
    gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(crate::gpu::with_common(source).into()) })
}

/// The bind group layout of one uniform buffer, for a layer's own uniforms.
pub fn uniform_layout(gpu: &Gpu, size: u64) -> wgpu::BindGroupLayout {
    gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("layer uniforms"), entries: &[crate::gpu::uniform_entry(0, size, false)] })
}

pub fn uniform_group(gpu: &Gpu, layout: &wgpu::BindGroupLayout, buffer: &wgpu::Buffer) -> wgpu::BindGroup {
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("layer uniforms"), layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }] })
}

/// A buffer that grows, written whole each time it changes.
pub struct DynamicBuffer {
    pub buffer: wgpu::Buffer,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl DynamicBuffer {
    pub fn new(gpu: &Gpu, label: &'static str, usage: wgpu::BufferUsages, size: u64) -> Self {
        let usage = usage | wgpu::BufferUsages::COPY_DST;
        DynamicBuffer { buffer: gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size.max(16), usage, mapped_at_creation: false }), usage, label }
    }

    /// Write `data`; returns whether the buffer was made again (bind groups over it are stale).
    pub fn write(&mut self, gpu: &Gpu, data: &[u8]) -> bool {
        let mut remade = false;
        if data.len() as u64 > self.buffer.size() {
            self.buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some(self.label), size: (data.len() as u64).next_power_of_two(), usage: self.usage, mapped_at_creation: false });
            remade = true;
        }
        if !data.is_empty() {
            gpu.queue.write_buffer(&self.buffer, 0, data);
        }
        remade
    }
}
