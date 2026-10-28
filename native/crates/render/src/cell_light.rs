use crate::gpu::Gpu;
use crate::lights::{linear, IsoLights};
use crate::memory::MEMORY_CELLS;
use bytemuck::{Pod, Zeroable};
use half::f16;
use isorun_scene::{CellKind, Scene, SceneCell, Visibility};

const OPEN_WEIGHT: f32 = 1.0;
const WALL_WEIGHT: f32 = 0.004;
const HERO_HEIGHT: f32 = 1.6;
const LEAST_SIGHT_REACH: f32 = 2.5;
const HALO_LIGHT: f32 = 6.0;
const SIGHT_SECONDS: f64 = 0.12;
const SETTLED: f64 = 0.002;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, Default)]
pub struct CellLightUniform {
    pub hero: [f32; 4],
    pub hero_colour: [f32; 4],
    pub memory: [f32; 4],
    pub fill: [f32; 4],
    pub halo: [f32; 4],
}

type Rgb = [f32; 3];

fn cell_f32(v: i32) -> f32 {
    f32::from(i16::try_from(v).unwrap_or(i16::MAX))
}

fn memory_cells() -> i32 {
    i32::try_from(MEMORY_CELLS).unwrap_or(i32::MAX)
}

fn texel_of(x: i32, y: i32) -> usize {
    let (n, wrapped) = (memory_cells(), |v: i32| usize::try_from(v.rem_euclid(memory_cells())).unwrap_or(0));
    wrapped(y) * usize::try_from(n).unwrap_or(0) + wrapped(x)
}

fn scaled(c: Rgb, k: f32) -> Rgb {
    c.map(|v| v * k)
}

fn vivid(c: Rgb) -> Rgb {
    let most = c[0].max(c[1]).max(c[2]);
    if most <= 0.0 {
        return [0.0; 3];
    }
    scaled(c, 1.0 / most)
}

fn is_open(c: &SceneCell) -> bool {
    !c.occluder && c.kind != CellKind::Unknown
}

fn sight_texels(scene: &Scene) -> (Vec<[f32; 4]>, f32) {
    let mut texels = vec![[0.0; 4]; MEMORY_CELLS * MEMORY_CELLS];
    let mut reach = LEAST_SIGHT_REACH;
    let (px, py) = scene.player;
    let on = |b: bool| f32::from(u8::from(b));
    for c in scene.cells.values() {
        if c.visibility == Visibility::Unseen {
            continue;
        }
        let open = is_open(c);
        let seen = c.visibility == Visibility::Visible;
        if seen && open {
            reach = reach.max(cell_f32(c.x - px).hypot(cell_f32(c.y - py)) + 0.5);
        }
        let weight = if open { OPEN_WEIGHT } else { WALL_WEIGHT };
        texels[texel_of(c.x, c.y)] = [on(seen) * weight, on(c.flags.halo) * weight, on(c.flags.umbra) * weight, weight];
    }
    (texels, reach)
}

fn half_texels(texels: &[[f64; 4]]) -> Vec<u16> {
    texels.iter().flat_map(|t| t.map(|c| f16::from_f64(c).to_bits())).collect()
}

pub struct CellLight {
    pub buffer: wgpu::Buffer,
    texture: wgpu::Texture,
    target: Vec<[f64; 4]>,
    shown: Vec<[f64; 4]>,
    settled: bool,
    dirty: bool,
    updated_for: Option<u64>,
    sight_reach: f32,
}

impl CellLight {
    pub fn new(gpu: &Gpu) -> Self {
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell light"),
            size: u64::try_from(std::mem::size_of::<CellLightUniform>()).unwrap_or(0),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let n = u32::try_from(MEMORY_CELLS).unwrap_or(1);
        let empty = vec![[0.0; 4]; MEMORY_CELLS * MEMORY_CELLS];
        let texture = gpu.texture_with("cell sight", n, n, wgpu::TextureFormat::Rgba16Float, bytemuck::cast_slice(&half_texels(&empty)));
        CellLight { buffer, texture, target: empty.clone(), shown: empty, settled: true, dirty: false, updated_for: None, sight_reach: LEAST_SIGHT_REACH }
    }

    pub fn sight_texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn easing(&self) -> bool {
        !self.settled
    }

    pub fn snap(&mut self) {
        self.shown.clone_from(&self.target);
        self.settled = true;
        self.dirty = true;
    }

    pub fn update(&mut self, scene: &Scene, revision: u64) {
        if self.updated_for == Some(revision) {
            return;
        }
        self.updated_for = Some(revision);
        let (texels, reach) = sight_texels(scene);
        self.sight_reach = reach;
        let target: Vec<[f64; 4]> = texels.iter().map(|t| t.map(f64::from)).collect();
        if target != self.target {
            self.target = target;
            self.settled = false;
        }
    }

    pub fn advance(&mut self, gpu: &Gpu, dt: f64) {
        if !self.settled {
            let ease = 1.0 - (-dt / SIGHT_SECONDS).exp();
            let mut moving = false;
            for (shown, target) in self.shown.iter_mut().zip(&self.target) {
                for (s, t) in shown.iter_mut().zip(target) {
                    let gap = t - *s;
                    if gap.abs() <= SETTLED {
                        *s = *t;
                    } else {
                        *s += gap * ease;
                        moving = true;
                    }
                }
            }
            self.settled = !moving;
            self.dirty = true;
        }
        if self.dirty {
            self.dirty = false;
            let n = u32::try_from(MEMORY_CELLS).unwrap_or(1);
            gpu.write(&self.texture, n, n, bytemuck::cast_slice(&half_texels(&self.shown)));
        }
    }

    pub fn uniform(&self, lights: &IsoLights, hero: Option<(f32, f32)>) -> CellLightUniform {
        let sight = lights.sight;
        let [r, g, b] = scaled(linear(sight.colour), sight.strength);
        let (hero, hero_colour) = match hero {
            Some((x, y)) => ([x + 0.5, HERO_HEIGHT, y + 0.5, self.sight_reach], [r, g, b, sight.light_at_sight_edge]),
            None => ([0.0; 4], [0.0; 4]),
        };
        let [mr, mg, mb] = scaled(vivid(linear(sight.memory_colour)), sight.memory_level);
        let [hr, hg, hb] = scaled(linear(sight.halo), HALO_LIGHT);
        CellLightUniform {
            hero,
            hero_colour,
            memory: [mr, mg, mb, 0.0],
            fill: [sight.fill_kept, sight.flicker, 0.0, 0.0],
            halo: [hr, hg, hb, sight.umbra_darkness],
        }
    }

    pub fn write_uniform(&self, gpu: &Gpu, uniform: &CellLightUniform) {
        gpu.queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(uniform));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use isorun_scene::{CellKind, Scene, SceneCell};

    fn cell(x: i32, y: i32, kind: CellKind, visibility: Visibility) -> SceneCell {
        SceneCell { x, y, kind, visibility, occluder: kind == CellKind::Wall, ..SceneCell::default() }
    }

    fn row(cells: &str, visibility: impl Fn(i32) -> Visibility) -> Scene {
        let mut scene = Scene::default();
        for (x, c) in (0i32..).zip(cells.chars()) {
            scene.cells.insert(isorun_scene::cell_key(x, 0), cell(x, 0, if c == '#' { CellKind::Wall } else { CellKind::Floor }, visibility(x)));
        }
        scene
    }

    #[test]
    fn sight_marks_what_crawl_shows_and_reaches_its_edge() {
        // given
        let mut scene = row("#....#", |x| if x < 3 { Visibility::Visible } else { Visibility::Remembered });
        scene.player = (1, 0);

        // when
        let (texels, reach) = sight_texels(&scene);

        // then
        assert_eq!(texels[texel_of(2, 0)], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(texels[texel_of(4, 0)], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(texels[texel_of(0, 0)][3], WALL_WEIGHT);
        assert_eq!(reach, LEAST_SIGHT_REACH);
    }
}
