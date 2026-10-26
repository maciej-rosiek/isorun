//! The light of every wall torch on the level, baked into a map over the cells: the renderer has
//! only `TORCH_LIGHTS` real point lights, at the torches nearest the middle of the view, so the
//! torches further away light their part of the level from this map. A texel holds the torches'
//! attenuated light (alpha) and the way across to them, weighted by it (red and blue), where no
//! wall stands between the torch and the texel's cell; the shaders take the height from the torch's. A torch with a real light takes its share of the map
//! as its light fades in, so the two never add up.

use crate::gpu::Gpu;
use crate::memory::MEMORY_CELLS;
use crate::renderer::Torch;
use glam::Vec3;
use half::f16;
use isorun_scene::{CellKind, Scene};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub const TEXELS_PER_CELL: usize = 4;
const SIZE: usize = MEMORY_CELLS * TEXELS_PER_CELL;
const LIT_HEIGHT: f32 = 0.35;
const SIGHT_POINTS: [(f32, f32); 5] = [(0.0, 0.0), (-0.3, -0.3), (0.3, -0.3), (-0.3, 0.3), (0.3, 0.3)];
const WEIGHT_STEPS: f32 = 32.0;

type TorchKey = (i32, i32, u32, u32);

fn key_of(torch: &Torch) -> TorchKey {
    (torch.cell.0, torch.cell.1, torch.position.x.to_bits(), torch.position.z.to_bits())
}

#[derive(Clone, Copy)]
struct Window {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl Window {
    fn union(self, other: Window) -> Window {
        Window { x0: self.x0.min(other.x0), y0: self.y0.min(other.y0), x1: self.x1.max(other.x1), y1: self.y1.max(other.y1) }
    }
}

struct Baked {
    walls: u64,
    window: Window,
    texels: Vec<[f32; 4]>,
    weight: f32,
}

pub struct TorchLightMap {
    pub texture: wgpu::Texture,
    sum: Vec<[f32; 4]>,
    torches: HashMap<TorchKey, Baked>,
    baked_for: Option<(u64, u32, u32)>,
    dirty: Option<Window>,
}

fn blocks(scene: &Scene, x: i32, y: i32) -> bool {
    scene.cell(x, y).is_none_or(|c| c.occluder || c.kind == CellKind::Unknown)
}

fn clear(scene: &Scene, from: (f32, f32), to: (f32, f32)) -> bool {
    let (mut x, mut y) = (from.0.floor() as i32, from.1.floor() as i32);
    let (end_x, end_y) = (to.0.floor() as i32, to.1.floor() as i32);
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let step_x = if dx > 0.0 { 1 } else { -1 };
    let step_y = if dy > 0.0 { 1 } else { -1 };
    let across_x = if dx != 0.0 { (1.0 / dx).abs() } else { f32::INFINITY };
    let across_y = if dy != 0.0 { (1.0 / dy).abs() } else { f32::INFINITY };
    let mut next_x = if dx > 0.0 { (x as f32 + 1.0 - from.0) * across_x } else if dx < 0.0 { (from.0 - x as f32) * across_x } else { f32::INFINITY };
    let mut next_y = if dy > 0.0 { (y as f32 + 1.0 - from.1) * across_y } else if dy < 0.0 { (from.1 - y as f32) * across_y } else { f32::INFINITY };
    loop {
        if next_x.min(next_y) >= 1.0 {
            return true;
        }
        if next_x < next_y {
            x += step_x;
            next_x += across_x;
        } else {
            y += step_y;
            next_y += across_y;
        }
        if (x, y) == (end_x, end_y) {
            return true;
        }
        if blocks(scene, x, y) {
            return false;
        }
    }
}

fn attenuation(d: f32, cutoff: f32, decay: f32) -> f32 {
    let falloff = 1.0 / d.powf(decay).max(0.01);
    if cutoff > 0.0 {
        let c = (1.0 - (d / cutoff).powi(4)).clamp(0.0, 1.0);
        return falloff * c * c;
    }
    falloff
}

fn bake(scene: &Scene, torch: &Torch, reach: f32, decay: f32, walls: u64, window: Window) -> Baked {
    let (cx0, cy0) = (window.x0 / TEXELS_PER_CELL, window.y0 / TEXELS_PER_CELL);
    let (cx1, cy1) = (window.x1 / TEXELS_PER_CELL, window.y1 / TEXELS_PER_CELL);
    let from = (torch.position.x, torch.position.z);
    let columns = cx1 - cx0;
    let mut sight = vec![0.0f32; columns * (cy1 - cy0)];
    for cy in cy0..cy1 {
        for cx in cx0..cx1 {
            let seen = SIGHT_POINTS.iter().filter(|(ox, oy)| clear(scene, from, (cx as f32 + 0.5 + ox, cy as f32 + 0.5 + oy))).count();
            sight[(cy - cy0) * columns + cx - cx0] = seen as f32 / SIGHT_POINTS.len() as f32;
        }
    }
    let width = window.x1 - window.x0;
    let mut texels = vec![[0.0f32; 4]; width * (window.y1 - window.y0)];
    for ty in window.y0..window.y1 {
        for tx in window.x0..window.x1 {
            let seen = sight[(ty / TEXELS_PER_CELL - cy0) * columns + tx / TEXELS_PER_CELL - cx0];
            if seen <= 0.0 {
                continue;
            }
            let at = Vec3::new((tx as f32 + 0.5) / TEXELS_PER_CELL as f32, LIT_HEIGHT, (ty as f32 + 0.5) / TEXELS_PER_CELL as f32);
            let to_torch = torch.position - at;
            let light = attenuation(to_torch.length(), reach, decay) * seen;
            if light <= 0.0 {
                continue;
            }
            texels[(ty - window.y0) * width + tx - window.x0] = [to_torch.x * light, 0.0, to_torch.z * light, light];
        }
    }
    Baked { walls, window, texels, weight: 0.0 }
}

impl TorchLightMap {
    pub fn new(gpu: &Gpu) -> Self {
        let texture = gpu.texture_with("torch light", SIZE as u32, SIZE as u32, wgpu::TextureFormat::Rgba16Float, &vec![0u8; SIZE * SIZE * 8]);
        TorchLightMap { texture, sum: vec![[0.0; 4]; SIZE * SIZE], torches: HashMap::new(), baked_for: None, dirty: None }
    }

    fn add(&mut self, baked: &Baked, amount: f32) {
        if amount == 0.0 {
            return;
        }
        let w = baked.window;
        let width = w.x1 - w.x0;
        for ty in w.y0..w.y1 {
            for tx in w.x0..w.x1 {
                let t = baked.texels[(ty - w.y0) * width + tx - w.x0];
                let s = &mut self.sum[ty * SIZE + tx];
                for c in 0..4 {
                    s[c] += t[c] * amount;
                }
            }
        }
        self.dirty = Some(self.dirty.map_or(w, |d| d.union(w)));
    }

    pub fn bake(&mut self, scene: &Scene, torches: &[Torch], version: u64, reach: f32, decay: f32) {
        let baked_for = Some((version, reach.to_bits(), decay.to_bits()));
        if baked_for == self.baked_for {
            return;
        }
        if self.baked_for.is_some_and(|(_, r, d)| (r, d) != (reach.to_bits(), decay.to_bits())) {
            self.clear();
        }
        self.baked_for = baked_for;
        let cells = reach.ceil() as i32 + 1;
        let mut kept: HashMap<TorchKey, Baked> = HashMap::new();
        for torch in torches {
            let key = key_of(torch);
            let (tx, ty) = (torch.position.x.floor() as i32, torch.position.z.floor() as i32);
            let limit = |v: i32| v.clamp(0, MEMORY_CELLS as i32) as usize;
            let (cx0, cy0, cx1, cy1) = (limit(tx - cells), limit(ty - cells), limit(tx + cells + 1), limit(ty + cells + 1));
            if cx0 >= cx1 || cy0 >= cy1 {
                continue;
            }
            let mut hasher = DefaultHasher::new();
            for cy in cy0..cy1 {
                for cx in cx0..cx1 {
                    blocks(scene, cx as i32, cy as i32).hash(&mut hasher);
                }
            }
            let walls = hasher.finish();
            if let Some(old) = self.torches.remove(&key) {
                if old.walls == walls {
                    kept.insert(key, old);
                    continue;
                }
                self.add(&old, -old.weight);
            }
            let window = Window { x0: cx0 * TEXELS_PER_CELL, y0: cy0 * TEXELS_PER_CELL, x1: cx1 * TEXELS_PER_CELL, y1: cy1 * TEXELS_PER_CELL };
            kept.insert(key, bake(scene, torch, reach, decay, walls, window));
        }
        let gone: Vec<Baked> = std::mem::take(&mut self.torches).into_values().collect();
        for old in gone {
            self.add(&old, -old.weight);
        }
        self.torches = kept;
        let fresh: Vec<TorchKey> = self.torches.iter().filter(|(_, b)| b.weight == 0.0).map(|(k, _)| *k).collect();
        for key in fresh {
            if let Some(mut baked) = self.torches.remove(&key) {
                baked.weight = 1.0;
                self.add(&baked, 1.0);
                self.torches.insert(key, baked);
            }
        }
    }

    pub fn weigh(&mut self, lit: &[(Torch, f32)]) {
        let shares: HashMap<TorchKey, f32> = lit.iter().map(|(t, fade)| (key_of(t), ((1.0 - fade) * WEIGHT_STEPS).round() / WEIGHT_STEPS)).collect();
        let keys: Vec<TorchKey> = self.torches.keys().copied().collect();
        for key in keys {
            let share = shares.get(&key).copied().unwrap_or(1.0);
            let Some(mut baked) = self.torches.remove(&key) else { continue };
            if baked.weight != share {
                self.add(&baked, share - baked.weight);
                baked.weight = share;
            }
            self.torches.insert(key, baked);
        }
    }

    fn clear(&mut self) {
        self.torches.clear();
        self.sum.iter_mut().for_each(|s| *s = [0.0; 4]);
        self.dirty = Some(Window { x0: 0, y0: 0, x1: SIZE, y1: SIZE });
    }

    pub fn upload(&mut self, gpu: &Gpu) {
        let Some(w) = self.dirty.take() else { return };
        let width = w.x1 - w.x0;
        let mut data: Vec<u16> = Vec::with_capacity(width * (w.y1 - w.y0) * 4);
        for ty in w.y0..w.y1 {
            for s in &self.sum[ty * SIZE + w.x0..ty * SIZE + w.x1] {
                let light = s[3].max(0.0);
                data.extend([s[0], s[1], s[2]].map(|c| f16::from_f32(if light > 0.0 { c } else { 0.0 }).to_bits()));
                data.push(f16::from_f32(light).to_bits());
            }
        }
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d { x: w.x0 as u32, y: w.y0 as u32, z: 0 }, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&data),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width as u32 * 8), rows_per_image: Some((w.y1 - w.y0) as u32) },
            wgpu::Extent3d { width: width as u32, height: (w.y1 - w.y0) as u32, depth_or_array_layers: 1 },
        );
    }
}
