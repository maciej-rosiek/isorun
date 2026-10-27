//! The screen effects after the scene pass (the port of `post.ts`): GTAO, the tilt-shift blur at
//! half the size, the bloom of bright light and of what glows at a quarter of it, and the
//! composite with the painted filter, outlines, pixels, the grade, the vignette, the danger edge
//! and the tone mapping.

use crate::gpu::{uniform_entry, Gpu, DEPTH_FORMAT, HDR_FORMAT, NORMAL_FORMAT};
use crate::lights::{linear, IsoLights};
use bytemuck::{Pod, Zeroable};
use glam::Mat4;

const BLUR_SIGMA: f32 = 1.0;
const BLUR_STEP: f32 = (3.0 + 2.0 * 4.0) / (3.0 + 2.0 * BLUR_SIGMA);
/// three's GaussianBlurNode: a kernel of `3 + 2 * sigma` taps each side less the middle, weighed by a Gaussian as wide as the kernel.
const BLUR_KERNEL: f32 = 3.0 + 2.0 * BLUR_SIGMA;
const BLUR_RESOLUTION: f32 = 0.5;
const BLOOM_RESOLUTION: f32 = 0.25;
const BLOOM_MIPS: usize = 5;
const BLOOM_KERNELS: [f32; BLOOM_MIPS] = [3.0, 5.0, 7.0, 9.0, 11.0];
const BLOOM_FACTORS: [f32; BLOOM_MIPS] = [1.0, 0.8, 0.6, 0.4, 0.2];
const EMISSION_RADIUS: f32 = 0.6;
const AO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostUniform {
    ao: [f32; 4],
    edges: [f32; 4],
    outline_colour: [f32; 4],
    bloom: [f32; 4],
    tilt: [f32; 4],
    grade: [f32; 4],
    shadows: [f32; 4],
    highlights: [f32; 4],
    flags0: [f32; 4],
    flags1: [f32; 4],
    size: [f32; 4],
    background: [f32; 4],
    proj: [[f32; 4]; 4],
    inv_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlurUniform {
    step: [f32; 4],
    params: [f32; 4],
}

const BLUR_STRIDE: u64 = 256;

/// What the composite needs to know of the frame beyond the look.
pub struct PostFrame {
    pub proj: Mat4,
    /// The pixel look's square size, times the zoom, and the map's origin on the screen in pixels.
    pub pixel_scale: f32,
    pub pixel_origin: [f32; 2],
    pub danger: f32,
    pub emission: bool,
    pub perspective: bool,
}

struct Chain {
    /// The high pass, then each mip's two blurs.
    high: wgpu::Texture,
    mips: Vec<(wgpu::Texture, wgpu::Texture)>,
    sum: wgpu::Texture,
}

pub struct Targets {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    pub colour: wgpu::Texture,
    pub normal: wgpu::Texture,
    pub emission: wgpu::Texture,
    pub depth: wgpu::Texture,
    /// The multisampled scene, resolved into the textures above.
    pub ms: Option<(wgpu::Texture, wgpu::Texture, wgpu::Texture, wgpu::Texture)>,
    ao: wgpu::Texture,
    blur: (wgpu::Texture, wgpu::Texture),
    bloom: Chain,
    glow: Chain,
}

pub struct Post {
    uniform: wgpu::Buffer,
    blur_uniform: wgpu::Buffer,
    sum_uniform: wgpu::Buffer,
    frame_group: wgpu::BindGroup,
    blur_group: wgpu::BindGroup,
    sum_groups: [wgpu::BindGroup; 2],
    ao_layout: wgpu::BindGroupLayout,
    source_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    sum_layout: wgpu::BindGroupLayout,
    ao_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    high_pass_pipeline: wgpu::RenderPipeline,
    sum_pipeline: wgpu::RenderPipeline,
    depth_copy: Option<(wgpu::BindGroupLayout, wgpu::RenderPipeline)>,
    composite: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    composite_pl: wgpu::PipelineLayout,
    module: wgpu::ShaderModule,
    pub targets: Option<Targets>,
    ao_resolution: f32,
    pub ao_samples: u32,
    blur_scratch: Vec<u8>,
    ao_noise: wgpu::Texture,
    /// Shows one of the effects' images alone: 1 the glow's bloom, 2 the ambient occlusion, 3 the normals, 4 the tilt-shift blur.
    pub debug: u32,
}

fn tex_entry(binding: u32, sample: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type: sample, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        count: None,
    }
}

fn view(t: &wgpu::Texture) -> wgpu::TextureView {
    t.create_view(&wgpu::TextureViewDescriptor::default())
}

impl Post {
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/post.wgsl").into()),
        });
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post frame"),
            entries: &[
                wgpu::BindGroupLayoutEntry { visibility: wgpu::ShaderStages::FRAGMENT, ..uniform_entry(0, std::mem::size_of::<PostUniform>() as u64, false) },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // the depth's: WebGL2 (OpenGL ES 3) takes a depth texture whose own filter is linear,
                // as wgpu leaves it, for incomplete unless a nearest sampler is bound with it, and
                // then reads 0 everywhere
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let ao_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ao"),
            entries: &[
                tex_entry(0, wgpu::TextureSampleType::Float { filterable: false }),
                tex_entry(1, float),
                tex_entry(2, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
        let source_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("source"), entries: &[tex_entry(0, float)] });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite"),
            entries: &[
                tex_entry(0, float),
                tex_entry(1, float),
                tex_entry(2, wgpu::TextureSampleType::Float { filterable: false }),
                tex_entry(3, float),
                tex_entry(4, float),
                tex_entry(5, float),
                tex_entry(6, float),
            ],
        });
        let sum_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom sum"),
            entries: &(0..5).map(|i| tex_entry(i, float)).collect::<Vec<_>>(),
        });
        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur"),
            entries: &[wgpu::BindGroupLayoutEntry { visibility: wgpu::ShaderStages::FRAGMENT, ..uniform_entry(0, 32, true) }],
        });
        let sum_uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom sum uniform"),
            entries: &[wgpu::BindGroupLayoutEntry { visibility: wgpu::ShaderStages::FRAGMENT, ..uniform_entry(0, 32, false) }],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post"),
            size: std::mem::size_of::<PostUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blur_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blurs"),
            size: BLUR_STRIDE * 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sum_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bloom sums"),
            size: BLUR_STRIDE * 2,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post frame"),
            layout: &frame_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&gpu.linear_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&gpu.nearest_sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&gpu.nearest_sampler) },
            ],
        });
        let blur_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur"),
            layout: &blur_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &blur_uniform, offset: 0, size: std::num::NonZeroU64::new(32) }),
            }],
        });
        let sum_group = |offset: u64| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom sum"),
                layout: &sum_uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &sum_uniform, offset, size: std::num::NonZeroU64::new(32) }),
                }],
            })
        };
        let sum_groups = [sum_group(0), sum_group(BLUR_STRIDE)];
        let pl = |groups: &[&wgpu::BindGroupLayout]| {
            let groups: Vec<Option<&wgpu::BindGroupLayout>> = groups.iter().map(|g| Some(*g)).collect();
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("post"), bind_group_layouts: &groups, immediate_size: 0 })
        };
        let pipeline = |label: &str, layout: &wgpu::PipelineLayout, fs: &str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_full"), buffers: &[], compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    targets: &[Some(format.into())],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let ao_pipeline = pipeline("ao", &pl(&[&frame_layout, &ao_layout]), "fs_ao", AO_FORMAT);
        let blur_pl = pl(&[&frame_layout, &source_layout, &blur_layout]);
        let blur_pipeline = pipeline("blur", &blur_pl, "fs_blur", HDR_FORMAT);
        let high_pass_pipeline = pipeline("high pass", &blur_pl, "fs_high_pass", HDR_FORMAT);
        let sum_pipeline = pipeline("bloom sum", &pl(&[&frame_layout, &sum_layout, &sum_uniform_layout]), "fs_bloom_sum", HDR_FORMAT);
        let composite_pl = pl(&[&frame_layout, &composite_layout]);
        Post {
            uniform,
            blur_uniform,
            sum_uniform,
            frame_group,
            blur_group,
            sum_groups,
            ao_layout,
            source_layout,
            composite_layout,
            sum_layout,
            ao_pipeline,
            blur_pipeline,
            high_pass_pipeline,
            sum_pipeline,
            depth_copy: None,
            composite: None,
            composite_pl,
            module,
            targets: None,
            ao_resolution: 0.5,
            ao_samples: 8,
            blur_scratch: Vec::new(),
            ao_noise: gpu.texture_with("ao noise", 5, 5, wgpu::TextureFormat::Rgba8Unorm, &magic_square_noise(5)),
            debug: std::env::var("ISORUN_DEBUG_VIEW").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
        }
    }

    pub fn set_ao_resolution(&mut self, scale: f32) {
        if scale != self.ao_resolution {
            self.ao_resolution = scale;
            self.targets = None;
        }
    }

    pub fn ao_on(&self, lights: &IsoLights) -> bool {
        lights.ao.enabled && self.ao_resolution > 0.0
    }

    /// The scene's textures and the effects' at this size, made again when it changes.
    pub fn ensure(&mut self, gpu: &Gpu, width: u32, height: u32, samples: u32) {
        if let Some(t) = &self.targets {
            if t.width == width && t.height == height && t.samples == samples {
                return;
            }
        }
        let attach = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let tex = |label: &'static str, w: u32, h: u32, format| gpu.texture(label, w, h, format, 1, attach);
        let chain = |label: &'static str| {
            let (bw, bh) = (((width as f32 * BLOOM_RESOLUTION).round() as u32).max(1), ((height as f32 * BLOOM_RESOLUTION).round() as u32).max(1));
            let mut mips = Vec::new();
            let (mut w, mut h) = ((bw / 2).max(1), (bh / 2).max(1));
            for _ in 0..BLOOM_MIPS {
                mips.push((tex(label, w, h, HDR_FORMAT), tex(label, w, h, HDR_FORMAT)));
                w = (w / 2).max(1);
                h = (h / 2).max(1);
            }
            Chain { high: tex(label, bw, bh, HDR_FORMAT), sum: tex(label, (bw / 2).max(1), (bh / 2).max(1), HDR_FORMAT), mips }
        };
        let (aw, ah) =
            (((width as f32 * self.ao_resolution.max(0.1)).round() as u32).max(1), ((height as f32 * self.ao_resolution.max(0.1)).round() as u32).max(1));
        let (hw, hh) = (((width as f32 * BLUR_RESOLUTION).round() as u32).max(1), ((height as f32 * BLUR_RESOLUTION).round() as u32).max(1));
        let ms = (samples > 1).then(|| {
            let t = |label, format| {
                gpu.texture(label, width, height, format, samples, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING)
            };
            (t("ms colour", HDR_FORMAT), t("ms normal", NORMAL_FORMAT), t("ms emission", HDR_FORMAT), t("ms depth", DEPTH_FORMAT))
        });
        self.targets = Some(Targets {
            width,
            height,
            samples,
            colour: tex("scene colour", width, height, HDR_FORMAT),
            normal: tex("scene normal", width, height, NORMAL_FORMAT),
            emission: tex("scene emission", width, height, HDR_FORMAT),
            depth: tex("scene depth", width, height, DEPTH_FORMAT),
            ms,
            ao: tex("ao", aw, ah, AO_FORMAT),
            blur: (tex("blur", hw, hh, HDR_FORMAT), tex("blur", hw, hh, HDR_FORMAT)),
            bloom: chain("bloom"),
            glow: chain("glow"),
        });
    }

    fn composite_pipeline(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        if let Some((f, p)) = &self.composite {
            if *f == format {
                return p.clone();
            }
        }
        let p = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite"),
            layout: Some(&self.composite_pl),
            vertex: wgpu::VertexState { module: &self.module, entry_point: Some("vs_full"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &self.module,
                entry_point: Some("fs_composite"),
                targets: &[Some(format.into())],
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[("ENCODE_SRGB", if format.is_srgb() { 0.0 } else { 1.0 })],
                    ..Default::default()
                },
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        self.composite = Some((format, p.clone()));
        p
    }

    fn pass<'e>(encoder: &'e mut wgpu::CommandEncoder, label: &str, target: &wgpu::TextureView) -> wgpu::RenderPass<'e> {
        Self::pass_clear(encoder, label, target, wgpu::Color::BLACK)
    }

    fn pass_clear<'e>(encoder: &'e mut wgpu::CommandEncoder, label: &str, target: &wgpu::TextureView, clear: wgpu::Color) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    fn source_group(&self, device: &wgpu::Device, t: &wgpu::Texture) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("source"),
            layout: &self.source_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view(t)) }],
        })
    }

    fn blur_params(&mut self, step: [f32; 2], sigma: f32, taps: f32, pass: [f32; 4]) -> u32 {
        let offset = self.blur_scratch.len() as u32;
        self.blur_scratch.extend_from_slice(bytemuck::bytes_of(&BlurUniform { step: [step[0], step[1], sigma, taps], params: pass }));
        self.blur_scratch.resize(offset as usize + BLUR_STRIDE as usize, 0);
        offset
    }

    fn blur(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::Texture,
        tmp: &wgpu::Texture,
        dst: &wgpu::Texture,
        texels: f32,
        sigma: f32,
        taps: f32,
    ) {
        let (sw, sh) = (src.width() as f32, src.height() as f32);
        let (tw, th) = (tmp.width() as f32, tmp.height() as f32);
        // a step is a texel of the image the pass draws, as three's blurs measure it
        let across = self.blur_params([texels / tw, 0.0], sigma, taps, [0.0; 4]);
        let down = self.blur_params([0.0, texels / th], sigma, taps, [0.0; 4]);
        let _ = (sw, sh);
        let (g1, g2) = (self.source_group(device, src), self.source_group(device, tmp));
        {
            let v = view(tmp);
            let mut pass = Self::pass(encoder, "blur", &v);
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.set_bind_group(1, &g1, &[]);
            pass.set_bind_group(2, &self.blur_group, &[across]);
            pass.draw(0..3, 0..1);
        }
        let v = view(dst);
        let mut pass = Self::pass(encoder, "blur", &v);
        pass.set_pipeline(&self.blur_pipeline);
        pass.set_bind_group(0, &self.frame_group, &[]);
        pass.set_bind_group(1, &g2, &[]);
        pass.set_bind_group(2, &self.blur_group, &[down]);
        pass.draw(0..3, 0..1);
    }

    /// three's BloomNode: the high pass, five mips each blurred more, summed with their factors.
    fn bloom(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        chain: &Chain,
        threshold: f32,
        strength: f32,
        radius: f32,
        sum_slot: u64,
        queue: &wgpu::Queue,
    ) {
        let high = self.blur_params([0.0, 0.0], 1.0, 0.0, [threshold, 0.01, 1.0, 0.0]);
        let g = self.source_group(device, source);
        {
            let v = view(&chain.high);
            let mut pass = Self::pass(encoder, "high pass", &v);
            pass.set_pipeline(&self.high_pass_pipeline);
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.set_bind_group(1, &g, &[]);
            pass.set_bind_group(2, &self.blur_group, &[high]);
            pass.draw(0..3, 0..1);
        }
        let mut input = &chain.high;
        for (i, (tmp, out)) in chain.mips.iter().enumerate() {
            let k = BLOOM_KERNELS[i];
            // the first blur reads the larger image through its linear filter, as a down-sample
            self.blur(device, encoder, input, tmp, out, 1.0, k, k - 1.0);
            input = out;
        }
        let f: Vec<f32> = BLOOM_FACTORS.iter().map(|f| (f + (1.2 - f - f) * radius) * strength).collect();
        let data = [f[0], f[1], f[2], f[3], f[4], 0.0, 0.0, 0.0];
        queue.write_buffer(&self.sum_uniform, sum_slot, bytemuck::cast_slice(&data));
        let entries: Vec<wgpu::TextureView> = chain.mips.iter().map(|(_, out)| view(out)).collect();
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom sum"),
            layout: &self.sum_layout,
            entries: &entries
                .iter()
                .enumerate()
                .map(|(i, v)| wgpu::BindGroupEntry { binding: i as u32, resource: wgpu::BindingResource::TextureView(v) })
                .collect::<Vec<_>>(),
        });
        let sum_group = &self.sum_groups[(sum_slot / BLUR_STRIDE) as usize];
        let v = view(&chain.sum);
        let mut pass = Self::pass(encoder, "bloom sum", &v);
        pass.set_pipeline(&self.sum_pipeline);
        pass.set_bind_group(0, &self.frame_group, &[]);
        pass.set_bind_group(1, &group, &[]);
        pass.set_bind_group(2, sum_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Run the effects over the scene the scene pass drew, onto `output`.
    pub fn render(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        lights: &IsoLights,
        frame: &PostFrame,
        output: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        let device = &gpu.device;
        let Some(targets) = self.targets.take() else { return };
        self.blur_scratch.clear();
        let ao_on = lights.ao.enabled && self.ao_resolution > 0.0;
        let only_shaded = !lights.painted.enabled && !lights.outline.enabled && !lights.pixelate.enabled && !lights.bloom.enabled;
        let _ = only_shaded;
        let (w, h) = (targets.width as f32, targets.height as f32);
        let lin = |hex| {
            let l = linear(hex);
            [l[0], l[1], l[2], 1.0]
        };
        let flag = |b: bool| b as u8 as f32;
        let mut outline_colour = lin(lights.outline.colour);
        outline_colour[3] = lights.exposure;
        let uniform = PostUniform {
            ao: [lights.ao.intensity, lights.ao.radius, self.ao_samples as f32, self.debug as f32],
            edges: [lights.outline.thickness, lights.outline.strength, lights.painted.radius, lights.pixelate.size * frame.pixel_scale],
            outline_colour,
            bloom: [lights.bloom.strength, self.ao_resolution, lights.bloom.threshold, if frame.emission { 1.0 } else { 0.0 }],
            tilt: [lights.tilt_shift.blur, lights.tilt_shift.focus, lights.tilt_shift.falloff, frame.danger],
            grade: [lights.grade.saturation, lights.grade.vignette, frame.pixel_origin[0], frame.pixel_origin[1]],
            shadows: lin(lights.grade.shadows),
            highlights: lin(lights.grade.highlights),
            flags0: [flag(ao_on), flag(lights.outline.enabled), flag(lights.painted.enabled), flag(lights.pixelate.enabled)],
            flags1: [flag(lights.bloom.enabled), flag(lights.tilt_shift.enabled), flag(frame.emission), flag(frame.perspective)],
            size: [w, h, 1.0 / w, 1.0 / h],
            background: [lights.background_grain, frame.pixel_scale, 0.0, 0.0],
            proj: frame.proj.to_cols_array_2d(),
            inv_proj: frame.proj.inverse().to_cols_array_2d(),
        };
        gpu.queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        if ao_on {
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ao"),
                layout: &self.ao_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view(&targets.depth)) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view(&targets.normal)) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&view(&self.ao_noise)) },
                ],
            });
            let v = view(&targets.ao);
            // where nothing was drawn the occlusion stays as cleared: none
            let mut pass = Self::pass_clear(encoder, "ao", &v, wgpu::Color::WHITE);
            pass.set_pipeline(&self.ao_pipeline);
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.set_bind_group(1, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        if lights.tilt_shift.enabled {
            self.blur(device, encoder, &targets.colour, &targets.blur.0, &targets.blur.1, lights.tilt_shift.blur * BLUR_STEP, BLUR_KERNEL, BLUR_KERNEL - 1.0);
        }
        if lights.bloom.enabled {
            self.bloom(device, encoder, &targets.colour, &targets.bloom, lights.bloom.threshold, lights.bloom.strength, lights.bloom.radius, 0, &gpu.queue);
        }
        if frame.emission {
            self.bloom(device, encoder, &targets.emission, &targets.glow, 0.0, 1.0, EMISSION_RADIUS, BLUR_STRIDE, &gpu.queue);
        }
        if !self.blur_scratch.is_empty() {
            let needed = self.blur_scratch.len() as u64;
            if needed > self.blur_uniform.size() {
                log::warn!("too many blur passes for the blur uniforms");
                self.blur_scratch.truncate(self.blur_uniform.size() as usize);
            }
            gpu.queue.write_buffer(&self.blur_uniform, 0, &self.blur_scratch);
        }
        let pipeline = self.composite_pipeline(device, format);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite"),
            layout: &self.composite_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view(&targets.colour)) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view(&targets.normal)) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&view(&targets.depth)) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&view(&targets.ao)) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&view(&targets.blur.1)) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&view(&targets.bloom.sum)) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&view(&targets.glow.sum)) },
            ],
        });
        {
            let mut pass = Self::pass(encoder, "composite", output);
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &self.frame_group, &[]);
            pass.set_bind_group(1, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.targets = Some(targets);
    }
}

/// three's GTAO noise: a magic square of angles, one rotation a texel.
fn magic_square_noise(size: usize) -> Vec<u8> {
    let n = if size % 2 == 0 { size + 1 } else { size };
    let square = magic_square(n);
    let count = square.len();
    let mut data = vec![0u8; count * 4];
    for (i, &v) in square.iter().enumerate() {
        let angle = 2.0 * std::f64::consts::PI * v as f64 / count as f64;
        data[i * 4] = ((angle.cos() * 0.5 + 0.5) * 255.0) as u8;
        data[i * 4 + 1] = ((angle.sin() * 0.5 + 0.5) * 255.0) as u8;
        data[i * 4 + 2] = 127;
        data[i * 4 + 3] = 255;
    }
    data
}

fn magic_square(n: usize) -> Vec<usize> {
    let count = n * n;
    let mut square = vec![0usize; count];
    let (mut i, mut j) = ((n / 2) as isize, (n - 1) as isize);
    let mut num = 1;
    while num <= count {
        if i == -1 && j == n as isize {
            j = n as isize - 2;
            i = 0;
        } else {
            if j == n as isize {
                j = 0;
            }
            if i < 0 {
                i = n as isize - 1;
            }
        }
        if square[i as usize * n + j as usize] != 0 {
            j -= 2;
            i += 1;
            continue;
        }
        square[i as usize * n + j as usize] = num;
        num += 1;
        j += 1;
        i -= 1;
    }
    square
}
