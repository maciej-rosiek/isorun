//! The GPU objects every frame uses: the bind group layouts, the samplers, the shaders and the
//! pipelines of the level's materials.

use crate::mesh::Vertex;
use crate::uniforms::{DrawUniform, FrameUniform};

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The scene pass's outputs, in order: the colour, the view normal (alpha 0 marks the overlays),
/// and the light that glows.
pub const SCENE_TARGETS: [wgpu::TextureFormat; 3] = [HDR_FORMAT, NORMAL_FORMAT, HDR_FORMAT];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    Ground,
    Sprite,
    Actor,
    Cap,
    Clay,
}

/// How a pass draws over what is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Blend {
    Opaque,
    Alpha,
    Additive,
}

pub fn blend_state(blend: Blend) -> Option<wgpu::BlendState> {
    match blend {
        Blend::Opaque => None,
        Blend::Alpha => Some(wgpu::BlendState::ALPHA_BLENDING),
        Blend::Additive => Some(wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        }),
    }
}

/// The scene targets for a pass that blends: the colour blends, the normal and the light keep what is under.
pub fn scene_targets(blend: Blend, keep_normal: bool) -> Vec<Option<wgpu::ColorTargetState>> {
    let keep = Some(wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    });
    vec![
        Some(wgpu::ColorTargetState { format: SCENE_TARGETS[0], blend: blend_state(blend), write_mask: wgpu::ColorWrites::ALL }),
        Some(wgpu::ColorTargetState {
            format: SCENE_TARGETS[1],
            blend: if keep_normal { keep } else { blend_state(blend) },
            write_mask: wgpu::ColorWrites::ALL,
        }),
        Some(wgpu::ColorTargetState {
            format: SCENE_TARGETS[2],
            blend: if blend == Blend::Opaque { None } else { blend_state(Blend::Additive) },
            write_mask: wgpu::ColorWrites::ALL,
        }),
    ]
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub frame_layout: wgpu::BindGroupLayout,
    pub page_layout: wgpu::BindGroupLayout,
    pub draw_layout: wgpu::BindGroupLayout,
    pub page_sampler: wgpu::Sampler,
    pub nearest_page_sampler: wgpu::Sampler,
    pub linear_sampler: wgpu::Sampler,
    pub nearest_sampler: wgpu::Sampler,
    pub repeat_sampler: wgpu::Sampler,
    pub world: wgpu::ShaderModule,
    pub max_texture: u32,
    /// Whether each of a pass's targets can blend its own way. WebGL2 (and OpenGL ES) cannot:
    /// there the layers blend every target alike, premultiplied, and their shaders shape what
    /// they write to each target so it comes out as it would (layers.rs `premultiplied_source`).
    pub independent_blend: bool,
    /// The OpenGL (WebGL2) backend, which takes a texture's kind from its shape (layers.rs).
    pub gl: bool,
}

fn texture_entry(binding: u32, sample: wgpu::TextureSampleType, dim: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type: sample, view_dimension: dim, multisampled: false },
        count: None,
    }
}

fn sampler_entry(binding: u32, kind: wgpu::SamplerBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Sampler(kind), count: None }
}

pub fn uniform_entry(binding: u32, size: u64, dynamic: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: dynamic, min_binding_size: std::num::NonZeroU64::new(size) },
        count: None,
    }
}

/// Source text with the shared WGSL in front of it.
pub fn with_common(source: &str) -> String {
    format!("{}\n{}", include_str!("shaders/common.wgsl"), source)
}

impl Gpu {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let d2 = wgpu::TextureViewDimension::D2;
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[
                uniform_entry(0, std::mem::size_of::<FrameUniform>() as u64, false),
                texture_entry(4, float, d2),
                texture_entry(5, float, d2),
                sampler_entry(6, wgpu::SamplerBindingType::Filtering),
                sampler_entry(7, wgpu::SamplerBindingType::Filtering),
                texture_entry(8, float, d2),
                sampler_entry(9, wgpu::SamplerBindingType::Filtering),
                texture_entry(10, float, d2),
            ],
        });
        let page_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("page"),
            entries: &[
                texture_entry(0, float, d2),
                texture_entry(1, float, d2),
                texture_entry(2, float, d2),
                sampler_entry(3, wgpu::SamplerBindingType::Filtering),
                sampler_entry(4, wgpu::SamplerBindingType::Filtering),
            ],
        });
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw"),
            entries: &[uniform_entry(0, std::mem::size_of::<DrawUniform>() as u64, true)],
        });
        let sampler = |label, filter: wgpu::FilterMode, mip: wgpu::MipmapFilterMode, address: wgpu::AddressMode, anisotropy: u16| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: address,
                address_mode_v: address,
                address_mode_w: address,
                mag_filter: filter,
                min_filter: filter,
                mipmap_filter: mip,
                anisotropy_clamp: anisotropy,
                ..Default::default()
            })
        };
        let page_sampler = sampler("page", wgpu::FilterMode::Linear, wgpu::MipmapFilterMode::Linear, wgpu::AddressMode::ClampToEdge, 4);
        let nearest_page_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("page pixels"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let linear_sampler = sampler("linear", wgpu::FilterMode::Linear, wgpu::MipmapFilterMode::Nearest, wgpu::AddressMode::ClampToEdge, 1);
        let nearest_sampler = sampler("nearest", wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Nearest, wgpu::AddressMode::ClampToEdge, 1);
        let repeat_sampler = sampler("repeat", wgpu::FilterMode::Linear, wgpu::MipmapFilterMode::Nearest, wgpu::AddressMode::Repeat, 1);
        let world = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world"),
            source: wgpu::ShaderSource::Wgsl(with_common(include_str!("shaders/world.wgsl")).into()),
        });
        let max_texture = device.limits().max_texture_dimension_2d;
        let gl = device.adapter_info().backend == wgpu::Backend::Gl;
        let independent_blend = !gl;
        Gpu {
            device,
            queue,
            frame_layout,
            page_layout,
            draw_layout,
            page_sampler,
            nearest_page_sampler,
            linear_sampler,
            nearest_sampler,
            repeat_sampler,
            world,
            max_texture,
            independent_blend,
            gl,
        }
    }

    /// A pipeline of the level's materials for the scene pass.
    pub fn world_pipeline(&self, material: Material, samples: u32) -> wgpu::RenderPipeline {
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world"),
            bind_group_layouts: &[Some(&self.frame_layout), Some(&self.page_layout), Some(&self.draw_layout)],
            immediate_size: 0,
        });
        let (vs, fs) = match material {
            Material::Ground => ("vs_main", "fs_ground"),
            Material::Sprite => ("vs_main", "fs_sprite"),
            Material::Actor => ("vs_actor", "fs_actor"),
            Material::Cap => ("vs_main", "fs_cap"),
            Material::Clay => ("vs_main", "fs_clay"),
        };
        let targets = scene_targets(Blend::Opaque, false);
        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(fs),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &self.world,
                entry_point: Some(vs),
                buffers: &[Some(Vertex::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState { module: &self.world, entry_point: Some(fs), targets: &targets, compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
            multiview_mask: None,
            cache: None,
        })
    }

    pub fn texture(&self, label: &str, w: u32, h: u32, format: wgpu::TextureFormat, samples: u32, usage: wgpu::TextureUsages) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }

    /// A small texture with these pixels.
    pub fn texture_with(&self, label: &str, w: u32, h: u32, format: wgpu::TextureFormat, data: &[u8]) -> wgpu::Texture {
        let t = self.texture(label, w, h, format, 1, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        self.write(&t, w, h, data);
        t
    }

    pub fn write(&self, t: &wgpu::Texture, w: u32, h: u32, data: &[u8]) {
        let bpp = t.format().block_copy_size(None).unwrap_or(4);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * bpp), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }
}
