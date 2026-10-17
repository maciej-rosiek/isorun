//! The wall torches (the port of `torches.ts` and `dust.ts`): the KayKit torch model at every torch
//! of the level, a glow on each flame, and dust drifting up in the light of the lit ones.

use crate::gpu::{Blend, Gpu, Material};
use crate::layers::{layer_pipeline, shader, uniform_group, uniform_layout, DynamicBuffer};
use crate::lights::linear;
use crate::mesh::{GpuMesh, Vertex};
use crate::renderer::{seeded, Cast, DrawItem, PageRef, Torch};
use crate::uniforms::DrawUniform;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::sync::Arc;

/// Kit pieces are authored on a 4-unit grid; a cell is 1.
pub const KIT_SCALE: f32 = 0.25;
const GLOW_SIZE: f32 = 0.45;
const MOTES_PER_TORCH: usize = 24;
const SPREAD: f32 = 0.45;
const RISE: f32 = 0.8;
const MOTE_SIZE: f32 = 0.09;
const OUT_FROM_WALL: f32 = 0.55;

/// Where a wall torch's model stands, and where its flame glows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TorchPlacement {
    pub model: Mat4,
    pub glow: Vec3,
}

/// The torch model: one mesh, its texture, and where the model puts it.
pub struct TorchModel {
    pub mesh: Arc<GpuMesh>,
    pub texture: wgpu::Texture,
    pub matrix: Mat4,
    pub colour: [f32; 3],
}

impl TorchModel {
    pub fn load(gpu: &Gpu, glb: &[u8]) -> Result<Self, String> {
        let (doc, buffers, images) = gltf::import_slice(glb).map_err(|e| e.to_string())?;
        let scene = doc.default_scene().or_else(|| doc.scenes().next()).ok_or("no scene")?;
        fn walk(node: gltf::Node, parent: Mat4) -> Option<(usize, Mat4)> {
            let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
            if let Some(mesh) = node.mesh() {
                return Some((mesh.index(), world));
            }
            node.children().find_map(|child| walk(child, world))
        }
        let (index, matrix) = scene.nodes().find_map(|n| walk(n, Mat4::IDENTITY)).ok_or("the model has no mesh")?;
        let mesh = doc.meshes().nth(index).ok_or("no mesh")?;
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut texture = None;
        let mut colour = [1.0; 3];
        for prim in mesh.primitives() {
            let reader = prim.reader(|b| Some(&buffers[b.index()]));
            let positions: Vec<[f32; 3]> = reader.read_positions().ok_or("no positions")?.collect();
            let normals: Vec<[f32; 3]> = reader.read_normals().map(|n| n.collect()).unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);
            let uvs: Vec<[f32; 2]> = reader.read_tex_coords(0).map(|t| t.into_f32().collect()).unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
            let base = vertices.len() as u32;
            for i in 0..positions.len() {
                vertices.push(Vertex { position: positions[i], uv: uvs[i], uv_rect: [0.0, 0.0, 1.0, 1.0], colour: [1.0; 3], normal: normals[i] });
            }
            match reader.read_indices() {
                Some(ix) => indices.extend(ix.into_u32().map(|i| i + base)),
                None => indices.extend(base..vertices.len() as u32),
            }
            let pbr = prim.material().pbr_metallic_roughness();
            let f = pbr.base_color_factor();
            colour = [f[0], f[1], f[2]];
            if texture.is_none() {
                if let Some(info) = pbr.base_color_texture() {
                    let img = &images[info.texture().source().index()];
                    texture = Some(image_rgba(gpu, img));
                }
            }
        }
        let texture = texture.unwrap_or_else(|| gpu.texture_with("torch", 1, 1, wgpu::TextureFormat::Rgba8UnormSrgb, &[255, 255, 255, 255]));
        for v in &mut vertices {
            v.colour = colour;
        }
        Ok(TorchModel { mesh: Arc::new(GpuMesh::new(&gpu.device, &vertices, &indices)), texture, matrix, colour })
    }
}

fn image_rgba(gpu: &Gpu, img: &gltf::image::Data) -> wgpu::Texture {
    let (w, h) = (img.width, img.height);
    let rgba: Vec<u8> = match img.format {
        gltf::image::Format::R8G8B8A8 => img.pixels.clone(),
        gltf::image::Format::R8G8B8 => img.pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        _ => vec![255; (w * h * 4) as usize],
    };
    gpu.texture_with("torch", w, h, wgpu::TextureFormat::Rgba8UnormSrgb, &rgba)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlowUniform {
    colour: [f32; 4],
    inner: [f32; 4],
    mode: [f32; 4],
}

const CORNERS: [[f32; 2]; 6] = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];

/// A batch of glowing sprites: a uniform and the instances.
struct Glows {
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    instances: DynamicBuffer,
    count: u32,
}

impl Glows {
    fn new(gpu: &Gpu, layout: &wgpu::BindGroupLayout, inner: u32, colour: u32, size: f32, dust: bool) -> Self {
        let uniform = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("glow"), size: 48, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let c = linear(colour);
        gpu.queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&GlowUniform { colour: [c[0], c[1], c[2], size], inner: srgb(inner), mode: [dust as u8 as f32, 0.0, 0.0, 0.0] }));
        let group = uniform_group(gpu, layout, &uniform);
        Glows { uniform, group, instances: DynamicBuffer::new(gpu, "glow instances", wgpu::BufferUsages::VERTEX, 1024), count: 0 }
    }
}

pub struct TorchLayer {
    placements: Vec<TorchPlacement>,
    layout: wgpu::BindGroupLayout,
    module: wgpu::ShaderModule,
    corners: wgpu::Buffer,
    pipeline: Option<wgpu::RenderPipeline>,
    flames: Glows,
    dust: Glows,
    dust_key: Vec<[u32; 3]>,
    dust_strength: f32,
}

impl TorchLayer {
    pub fn new(gpu: &Gpu) -> Self {
        use wgpu::util::DeviceExt;
        let layout = uniform_layout(gpu, 48);
        let module = shader(gpu, "glow", include_str!("shaders/glow.wgsl"));
        let corners = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("corners"), contents: bytemuck::cast_slice(&CORNERS), usage: wgpu::BufferUsages::VERTEX });
        // the texture's colour at the middle, times the material's
        let flames = Glows::new(gpu, &layout, 0xffbe6e, 0xffb060, GLOW_SIZE, false);
        let dust = Glows::new(gpu, &layout, 0xffdcb4, 0xffc890, MOTE_SIZE, true);
        TorchLayer { placements: Vec::new(), layout, module, corners, pipeline: None, flames, dust, dust_key: Vec::new(), dust_strength: -1.0 }
    }

    pub fn prepare(&mut self, gpu: &Gpu, samples: u32) {
        let buffers = [
            wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2] },
            wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Instance, attributes: &wgpu::vertex_attr_array![1 => Float32x4] },
        ];
        self.pipeline = Some(layer_pipeline(gpu, "glow", &self.module, "vs_glow", "fs_glow", &buffers, &[&self.layout], Blend::Additive, false, samples, true, Default::default()));
    }

    /// Draw these torches.
    pub fn set(&mut self, gpu: &Gpu, placements: &[TorchPlacement]) {
        self.placements = placements.to_vec();
        let data: Vec<[f32; 4]> = placements.iter().map(|p| [p.glow.x, p.glow.y, p.glow.z, 0.0]).collect();
        self.flames.instances.write(gpu, bytemuck::cast_slice(&data));
        self.flames.count = data.len() as u32;
    }

    pub fn draws(&self, out: &mut Vec<DrawItem>, model: &TorchModel) {
        for p in &self.placements {
            let mut uniform = DrawUniform::at(p.model, true);
            uniform.look[3] = 0.0;
            out.push(DrawItem { material: Material::Clay, page: PageRef::Torch, mesh: model.mesh.clone(), uniform, cast: Cast::None });
        }
    }

    /// Put the motes in the air before the lit torches, over the open cell each hangs over.
    pub fn place_dust(&mut self, gpu: &Gpu, torches: &[Option<Torch>]) {
        let lit: Vec<Vec3> = torches.iter().flatten().map(|t| Vec3::new(t.cell.0 as f32 + 0.5, t.position.y, t.cell.1 as f32 + 0.5).lerp(t.position, 1.0 - OUT_FROM_WALL)).collect();
        let key: Vec<[u32; 3]> = lit.iter().map(|p| [(p.x * 100.0).round() as u32, (p.y * 100.0).round() as u32, (p.z * 100.0).round() as u32]).collect();
        if key == self.dust_key {
            return;
        }
        self.dust_key = key;
        let mut random = seeded((lit.len() as u32).wrapping_mul(2654435761));
        let mut data: Vec<[f32; 4]> = Vec::new();
        for p in &lit {
            for _ in 0..MOTES_PER_TORCH {
                let angle = random() * std::f32::consts::TAU;
                let reach = random().sqrt() * SPREAD;
                data.push([p.x + angle.cos() * reach, p.y - RISE / 2.0, p.z + angle.sin() * reach, random() * 100.0]);
            }
        }
        self.dust.instances.write(gpu, bytemuck::cast_slice(&data));
        self.dust.count = data.len() as u32;
    }

    pub fn set_dust_strength(&mut self, gpu: &Gpu, strength: f32) {
        if strength == self.dust_strength {
            return;
        }
        self.dust_strength = strength;
        let c = linear(0xffc890);
        gpu.queue.write_buffer(&self.dust.uniform, 0, bytemuck::bytes_of(&GlowUniform { colour: [c[0], c[1], c[2], MOTE_SIZE], inner: srgb(0xffdcb4), mode: [1.0, strength, 0.0, 0.0] }));
    }

    fn draw_glows_of(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup, glows: &Glows) {
        let Some(pipeline) = &self.pipeline else { return };
        if glows.count == 0 {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, &glows.group, &[]);
        pass.set_vertex_buffer(0, self.corners.slice(..));
        pass.set_vertex_buffer(1, glows.instances.buffer.slice(..));
        pass.draw(0..6, 0..glows.count);
    }

    pub fn draw_glows(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        self.draw_glows_of(pass, frame, &self.flames);
    }

    pub fn draw_dust(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        if self.dust_strength > 0.0 {
            self.draw_glows_of(pass, frame, &self.dust);
        }
    }
}

fn srgb(hex: u32) -> [f32; 4] {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0, 1.0]
}
