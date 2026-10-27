//! The cells out of sight and the wet ground, one texel per cell (shading.ts `MemoryMask`): the
//! level's materials draw a remembered cell darker by it, so a step that changes what is in sight
//! uploads a small texture and leaves the level's geometry as it is.

use crate::gpu::Gpu;
use isorun_scene::{LiquidKind, Scene, Visibility, MEMORY_SHADE};

pub const MEMORY_CELLS: usize = 128;
const IN_SIGHT: u8 = 255;
const WET_REACH: f32 = 2.5;
const SHALLOW_SUBMERSION: u8 = 100;

fn wrapped_cell(v: i32) -> usize {
    v.rem_euclid(MEMORY_CELLS as i32) as usize
}

fn texel_of(x: i32, y: i32) -> usize {
    (wrapped_cell(y) * MEMORY_CELLS + wrapped_cell(x)) * 4
}

fn wetting(k: LiquidKind) -> bool {
    matches!(k, LiquidKind::Water | LiquidKind::Murky | LiquidKind::Bog | LiquidKind::Mud | LiquidKind::Slime | LiquidKind::Acid)
}

pub struct MemoryMask {
    cells: Vec<u8>,
    wet: Vec<u8>,
    wet_key: Vec<(i32, i32, LiquidKind, bool)>,
    pub shade: wgpu::Texture,
    pub wetness: wgpu::Texture,
}

impl MemoryMask {
    pub fn new(gpu: &Gpu) -> Self {
        let n = MEMORY_CELLS as u32;
        let cells = vec![IN_SIGHT; MEMORY_CELLS * MEMORY_CELLS * 4];
        let wet = vec![0u8; MEMORY_CELLS * MEMORY_CELLS * 4];
        let shade = gpu.texture_with("memory", n, n, wgpu::TextureFormat::Rgba8Unorm, &cells);
        let wetness = gpu.texture_with("wetness", n, n, wgpu::TextureFormat::Rgba8Unorm, &wet);
        MemoryMask { cells, wet, wet_key: Vec::new(), shade, wetness }
    }

    pub fn update(&mut self, gpu: &Gpu, scene: &Scene) {
        let remembered = (MEMORY_SHADE * IN_SIGHT as f32).round() as u8;
        let mut next = vec![IN_SIGHT; MEMORY_CELLS * MEMORY_CELLS * 4];
        for c in scene.cells.values() {
            if c.visibility == Visibility::Remembered {
                next[texel_of(c.x, c.y)] = remembered;
            }
        }
        if next != self.cells {
            self.cells = next;
            gpu.write(&self.shade, MEMORY_CELLS as u32, MEMORY_CELLS as u32, &self.cells);
        }
        let mut sources: Vec<(i32, i32, LiquidKind, bool)> = scene
            .cells
            .values()
            .filter_map(|c| c.liquid.filter(|l| wetting(l.kind) && c.visibility != Visibility::Unseen).map(|l| (c.x, c.y, l.kind, l.deep)))
            .collect();
        sources.sort_by_key(|s| (s.1, s.0));
        if sources == self.wet_key {
            return;
        }
        self.wet.iter_mut().for_each(|v| *v = 0);
        let reach = WET_REACH.ceil() as i32;
        for &(sx, sy, _, _) in &sources {
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let wet = ((1.0 - ((dx * dx + dy * dy) as f32).sqrt() / WET_REACH).max(0.0) * 255.0) as u8;
                    let i = texel_of(sx + dx, sy + dy);
                    self.wet[i] = self.wet[i].max(wet);
                }
            }
        }
        for &(x, y, kind, deep) in &sources {
            if matches!(kind, LiquidKind::Water | LiquidKind::Murky) {
                self.wet[texel_of(x, y) + 1] = if deep { 255 } else { SHALLOW_SUBMERSION };
            }
        }
        self.wet_key = sources;
        gpu.write(&self.wetness, MEMORY_CELLS as u32, MEMORY_CELLS as u32, &self.wet);
    }
}
