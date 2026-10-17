//! The pointer on the ground: the arrow from the player to the cell the stick points at, the route
//! the pointer's walk takes, and the squares on the target and the cursor.

use crate::gpu::{Blend, Gpu};
use crate::layers::{layer_pipeline, shader, DynamicBuffer};
use crate::plan::IsoAim;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use isorun_scene::{DIR8_DX, DIR8_DY};
use wgpu::util::DeviceExt;

const AIM_LIFT: f32 = 0.03;
const DOT_RADIUS: f32 = 0.08;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    colour: [f32; 4],
}

struct Shape {
    vertices: wgpu::Buffer,
    count: u32,
    instances: DynamicBuffer,
    drawn: u32,
}

fn shape(gpu: &Gpu, triangles: &[[f32; 2]]) -> Shape {
    Shape {
        vertices: gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("overlay"), contents: bytemuck::cast_slice(triangles), usage: wgpu::BufferUsages::VERTEX }),
        count: triangles.len() as u32,
        instances: DynamicBuffer::new(gpu, "overlay instances", wgpu::BufferUsages::VERTEX, 1024),
        drawn: 0,
    }
}

/// An arrow from the cell's centre along +x, one cell long (three's ShapeGeometry of the outline).
fn arrow() -> Vec<[f32; 2]> {
    let (a, b, c, d) = ([0.18, -0.05], [0.62, -0.05], [0.62, 0.05], [0.18, 0.05]);
    vec![a, b, c, a, c, d, [0.62, -0.13], [0.8, 0.0], [0.62, 0.13]]
}

/// A square frame around a cell.
fn square() -> Vec<[f32; 2]> {
    let (o, i) = (0.47, 0.4);
    let mut out = Vec::new();
    let mut quad = |a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]| out.extend([a, b, c, a, c, d]);
    quad([-o, -o], [o, -o], [o, -i], [-o, -i]);
    quad([-o, i], [o, i], [o, o], [-o, o]);
    quad([-o, -i], [-i, -i], [-i, i], [-o, i]);
    quad([i, -i], [o, -i], [o, i], [i, i]);
    out
}

fn dot() -> Vec<[f32; 2]> {
    (0..16)
        .flat_map(|i| {
            let a0 = i as f32 / 16.0 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / 16.0 * std::f32::consts::TAU;
            [[0.0, 0.0], [a0.cos() * DOT_RADIUS, a0.sin() * DOT_RADIUS], [a1.cos() * DOT_RADIUS, a1.sin() * DOT_RADIUS]]
        })
        .collect()
}

pub struct Overlay {
    module: wgpu::ShaderModule,
    pipeline: Option<wgpu::RenderPipeline>,
    arrow: Shape,
    square: Shape,
    dot: Shape,
}

type Route = (Vec<(i32, i32)>, (i32, i32), f32, [f32; 3]);
type Cursor = ((i32, i32), f32, [f32; 3]);

impl Overlay {
    pub fn new(gpu: &Gpu) -> Self {
        Overlay { module: shader(gpu, "overlay", include_str!("shaders/overlay.wgsl")), pipeline: None, arrow: shape(gpu, &arrow()), square: shape(gpu, &square()), dot: shape(gpu, &dot()) }
    }

    pub fn prepare(&mut self, gpu: &Gpu, samples: u32) {
        let buffers = [
            wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x2] },
            wgpu::VertexBufferLayout { array_stride: 80, step_mode: wgpu::VertexStepMode::Instance, attributes: &wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4] },
        ];
        let bias = wgpu::DepthBiasState { constant: -2, slope_scale: -2.0, clamp: 0.0 };
        self.pipeline = Some(layer_pipeline(gpu, "overlay", &self.module, "vs_overlay", "fs_overlay", &buffers, &[], Blend::Opaque, true, samples, false, bias));
    }

    /// Where the pointer's pieces are this frame.
    pub fn place(&mut self, gpu: &Gpu, player: (i32, i32), aim: Option<(IsoAim, [f32; 3])>, route: Option<Route>, cursor: Option<Cursor>) {
        let instance = |m: Mat4, c: [f32; 3]| Instance { model: m.to_cols_array_2d(), colour: [c[0], c[1], c[2], 1.0] };
        let mut arrows = Vec::new();
        let mut squares = Vec::new();
        let mut dots = Vec::new();
        if let Some((aim, colour)) = aim {
            let (dx, dy) = (DIR8_DX[aim.dir as usize] as f32, DIR8_DY[aim.dir as usize] as f32);
            let angle = dy.atan2(dx);
            let reach = (dx * dx + dy * dy).sqrt();
            let group = Mat4::from_translation(Vec3::new(player.0 as f32 + 0.5, AIM_LIFT, player.1 as f32 + 0.5)) * Mat4::from_rotation_y(-angle);
            arrows.push(instance(group * Mat4::from_scale(Vec3::new(reach, 1.0, 1.0)), colour));
            squares.push(instance(group * Mat4::from_translation(Vec3::new(reach, 0.0, 0.0)) * Mat4::from_rotation_y(angle), colour));
        }
        if let Some((steps, target, lift, colour)) = route {
            for (x, y) in steps {
                dots.push(instance(Mat4::from_translation(Vec3::new(x as f32 + 0.5, AIM_LIFT, y as f32 + 0.5)), colour));
            }
            squares.push(instance(Mat4::from_translation(Vec3::new(target.0 as f32 + 0.5, lift, target.1 as f32 + 0.5)), colour));
        }
        if let Some((at, lift, colour)) = cursor {
            squares.push(instance(Mat4::from_translation(Vec3::new(at.0 as f32 + 0.5, lift, at.1 as f32 + 0.5)), colour));
        }
        for (shape, list) in [(&mut self.arrow, arrows), (&mut self.square, squares), (&mut self.dot, dots)] {
            shape.instances.write(gpu, bytemuck::cast_slice(&list));
            shape.drawn = list.len() as u32;
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        let Some(pipeline) = &self.pipeline else { return };
        for shape in [&self.dot, &self.arrow, &self.square] {
            if shape.drawn == 0 {
                continue;
            }
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, frame, &[]);
            pass.set_vertex_buffer(0, shape.vertices.slice(..));
            pass.set_vertex_buffer(1, shape.instances.buffer.slice(..));
            pass.draw(0..shape.count, 0..shape.drawn);
        }
    }
}
