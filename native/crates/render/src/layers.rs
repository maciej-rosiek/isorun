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
    cell_lights: u32,
) -> wgpu::RenderPipeline {
    let mut layouts: Vec<Option<&wgpu::BindGroupLayout>> = vec![Some(&gpu.frame_layout)];
    layouts.extend(groups.iter().map(|g| Some(*g)));
    let layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &layouts, immediate_size: 0 });
    let premultiplied = !gpu.independent_blend && blend != Blend::Opaque;
    let targets = if premultiplied {
        // one blend for every target: the shader writes each so that it blends as its own would
        crate::gpu::SCENE_TARGETS
            .iter()
            .map(|f| {
                Some(wgpu::ColorTargetState { format: *f, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })
            })
            .collect()
    } else {
        scene_targets(blend, keep_normal)
    };
    let blending = [("BLEND_MODE", if blend == Blend::Additive { 2.0 } else { 1.0 }), ("KEEP_NORMAL", if keep_normal { 1.0 } else { 0.0 })];
    let lighting = [("CELL_LIGHTS", f64::from(cell_lights))];
    let constants: Vec<(&str, f64)> = blending.iter().filter(|_| premultiplied).chain(lighting.iter().filter(|_| cell_lights != 0)).copied().collect();
    let compilation_options = wgpu::PipelineCompilationOptions { constants: &constants, ..Default::default() };
    let buffers: Vec<Option<wgpu::VertexBufferLayout>> = buffers.iter().cloned().map(Some).collect();
    gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module, entry_point: Some(vs), buffers: &buffers, compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module, entry_point: Some(fs), targets: &targets, compilation_options }),
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(depth_write),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias,
        }),
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        multiview_mask: None,
        cache: None,
    })
}

pub fn shader(gpu: &Gpu, label: &str, source: &str) -> wgpu::ShaderModule {
    let source = if gpu.independent_blend { source.to_string() } else { premultiplied_source(source) };
    gpu.device
        .create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(crate::gpu::with_common(&source).into()) })
}

/// A layer shader for a device whose targets all blend alike (WebGL2): each fragment entry that
/// writes the scene's targets becomes a function, and a new entry of the same name writes its
/// output premultiplied for PREMULTIPLIED_ALPHA_BLENDING, shaped per target as `scene_targets`
/// blends it: the colour by the pass's blend, the normal kept under (alpha 0, colour 0) or
/// blended, the light added (alpha 0). The pipeline sets the pass's blend in its constants.
pub fn premultiplied_source(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + 2048);
    let mut wrappers = String::new();
    let mut rest = source;
    while let Some(at) = rest.find("@fragment") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "@fragment".len()..];
        let head_end = after.find('{').unwrap_or(after.len());
        let head = &after[..head_end];
        let parsed = (|| {
            let name_start = head.find("fn ")? + 3;
            let open = head.find('(')?;
            let close = head.rfind(')')?;
            let name = head[name_start..open].trim();
            head[close..].contains("FragmentOut").then_some((name, &head[open + 1..close]))
        })();
        match parsed {
            Some((name, params)) => {
                let params: Vec<&str> = params.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
                // the parameters without their attributes (an entry's builtins), and their names
                let bare: Vec<String> = params.iter().map(|p| strip_attributes(p)).collect();
                let names: Vec<&str> = bare.iter().map(|p| p.split(':').next().unwrap_or("").trim()).collect();
                out.push_str(&format!("fn {name}__shaded({}) -> FragmentOut ", bare.join(", ")));
                wrappers.push_str(&format!(
                    "\n@fragment\nfn {name}({}) -> FragmentOut {{\n    return premultiplied({name}__shaded({}));\n}}\n",
                    params.join(", "),
                    names.join(", ")
                ));
            }
            None => {
                out.push_str("@fragment");
                out.push_str(head);
            }
        }
        rest = &after[head_end..];
    }
    out.push_str(rest);
    out.push_str(
        r#"
override BLEND_MODE: f32 = 1.0;
override KEEP_NORMAL: f32 = 0.0;

fn premultiplied(o: FragmentOut) -> FragmentOut {
    var p: FragmentOut;
    let additive = BLEND_MODE > 1.5;
    let ca = o.colour.a;
    p.colour = vec4<f32>(o.colour.rgb * ca, select(ca, 0.0, additive));
    let na = o.normal.a;
    p.normal = select(vec4<f32>(o.normal.rgb * na, select(na, 0.0, additive)), vec4<f32>(0.0), KEEP_NORMAL > 0.5);
    p.emission = vec4<f32>(o.emission.rgb * o.emission.a, 0.0);
    return p;
}
"#,
    );
    out.push_str(&wrappers);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn premultiplied_wraps_the_scene_entries() {
        let src = "struct FragmentOut { @location(0) colour: vec4<f32>, };\n@fragment\nfn fs_a(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {\n    return x;\n}\n@fragment\nfn fs_b(in: VertexOut) -> @location(0) vec4<f32> { return y; }\n";
        let out = super::premultiplied_source(src);
        assert!(out.contains("fn fs_a__shaded(in: VertexOut, front: bool) -> FragmentOut {"));
        assert!(out.contains(
            "@fragment\nfn fs_a(in: VertexOut, @builtin(front_facing) front: bool) -> FragmentOut {\n    return premultiplied(fs_a__shaded(in, front));"
        ));
        assert!(out.contains("@fragment\nfn fs_b(in: VertexOut) -> @location(0) vec4<f32> {"));
    }
}

/// `@builtin(front_facing) front: bool` -> `front: bool`.
fn strip_attributes(param: &str) -> String {
    let mut p = param.trim();
    while p.starts_with('@') {
        p = match p.find(')') {
            Some(i) => p[i + 1..].trim_start(),
            None => p.split_once(' ').map(|(_, r)| r).unwrap_or("").trim_start(),
        };
    }
    p.to_string()
}

/// The bind group layout of one uniform buffer, for a layer's own uniforms.
pub fn uniform_layout(gpu: &Gpu, size: u64) -> wgpu::BindGroupLayout {
    gpu.device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("layer uniforms"), entries: &[crate::gpu::uniform_entry(0, size, false)] })
}

pub fn uniform_group(gpu: &Gpu, layout: &wgpu::BindGroupLayout, buffer: &wgpu::Buffer) -> wgpu::BindGroup {
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("layer uniforms"),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
    })
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
        DynamicBuffer {
            buffer: gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: size.max(16), usage, mapped_at_creation: false }),
            usage,
            label,
        }
    }

    /// Write `data`; returns whether the buffer was made again (bind groups over it are stale).
    pub fn write(&mut self, gpu: &Gpu, data: &[u8]) -> bool {
        let mut remade = false;
        if data.len() as u64 > self.buffer.size() {
            self.buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: (data.len() as u64).next_power_of_two(),
                usage: self.usage,
                mapped_at_creation: false,
            });
            remade = true;
        }
        if !data.is_empty() {
            gpu.queue.write_buffer(&self.buffer, 0, data);
        }
        remade
    }
}
