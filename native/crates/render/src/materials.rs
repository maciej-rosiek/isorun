use crate::gpu::{uniform_entry, with_common, Gpu, Material};
use crate::labels::{material_name, TileLabel};
use crate::lights::{linear, Hex, IsoLights};
use crate::memory::MEMORY_CELLS;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use half::f16;
use isorun_scene::{LiquidKind, Scene, TileId, Visibility};
use std::collections::HashMap;

const CLASS_STEP: u8 = 17;
const HEAT_REACH: f32 = 2.5;
const HERO_HEIGHT: f32 = 0.9;
const HEAT_CELLS: i32 = 3;
const PLAIN_WALL: f32 = 0.5;
const TORCH_WALL: f32 = 1.0;
const UNIFORM_SIZE: u64 = 160;
const WORLD_MATERIALS: [Material; 7] = [Material::Ground, Material::Feature, Material::Sprite, Material::Actor, Material::Cap, Material::Clay, Material::Mark];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialClass {
    Plain,
    Stone,
    Metal,
    Gold,
    Crystal,
    Glass,
    Ice,
    Plant,
    Molten,
    Arcane,
    Shadow,
    Slime,
}

impl MaterialClass {
    pub fn of(label: &TileLabel) -> MaterialClass {
        match material_name(label) {
            Some("stone" | "brick" | "rock" | "marble" | "dirt" | "sand") => MaterialClass::Stone,
            Some("metal") => MaterialClass::Metal,
            Some("gold") => MaterialClass::Gold,
            Some("crystal" | "gem") => MaterialClass::Crystal,
            Some("glass") => MaterialClass::Glass,
            Some("ice") => MaterialClass::Ice,
            Some("grass" | "moss" | "plant") => MaterialClass::Plant,
            Some("lava" | "fire") => MaterialClass::Molten,
            Some("magic" | "light") => MaterialClass::Arcane,
            Some("shadow") => MaterialClass::Shadow,
            Some("slime" | "mud") => MaterialClass::Slime,
            _ => MaterialClass::Plain,
        }
    }

    pub fn code(self) -> u8 {
        match self {
            MaterialClass::Plain => 0,
            MaterialClass::Stone => 1,
            MaterialClass::Metal => 2,
            MaterialClass::Gold => 3,
            MaterialClass::Crystal => 4,
            MaterialClass::Glass => 5,
            MaterialClass::Ice => 6,
            MaterialClass::Plant => 7,
            MaterialClass::Molten => 8,
            MaterialClass::Arcane => 9,
            MaterialClass::Shadow => 10,
            MaterialClass::Slime => 11,
        }
    }

    pub fn texel_alpha(self) -> u8 {
        self.code() * CLASS_STEP
    }

    pub fn of_texel_alpha(alpha: u8) -> MaterialClass {
        let code = (alpha + CLASS_STEP / 2) / CLASS_STEP;
        [
            MaterialClass::Plain,
            MaterialClass::Stone,
            MaterialClass::Metal,
            MaterialClass::Gold,
            MaterialClass::Crystal,
            MaterialClass::Glass,
            MaterialClass::Ice,
            MaterialClass::Plant,
            MaterialClass::Molten,
            MaterialClass::Arcane,
            MaterialClass::Shadow,
            MaterialClass::Slime,
        ]
        .into_iter()
        .find(|c| c.code() == code)
        .unwrap_or(MaterialClass::Plain)
    }
}

pub fn tile_class(label: Option<&TileLabel>) -> MaterialClass {
    label.map_or(MaterialClass::Plain, MaterialClass::of)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialSettings {
    pub hero_light: Hex,
    pub hero_strength: f32,
    pub hero_reach: f32,
    pub room: Hex,
    pub metal_shine: f32,
    pub metal_sharpness: f32,
    pub crystal_glints: f32,
    pub crystal_colours: f32,
    pub crystal_depth: f32,
    pub see_through: f32,
    pub glass_tint: f32,
    pub ice_gloss: f32,
    pub wet_gloss: f32,
    pub puddles: f32,
    pub water_reflection: f32,
    pub sway: f32,
    pub sway_speed: f32,
    pub fungus: Hex,
    pub fungus_glow: f32,
    pub magic_glow: f32,
    pub lava_heat: f32,
    pub heat_flicker: f32,
    pub corruption: f32,
}

pub const MATERIALS: MaterialSettings = MaterialSettings {
    hero_light: 0xffd2a0,
    hero_strength: 6.0,
    hero_reach: 6.0,
    room: 0x4a4038,
    metal_shine: 1.0,
    metal_sharpness: 90.0,
    crystal_glints: 1.5,
    crystal_colours: 0.6,
    crystal_depth: 0.2,
    see_through: 0.55,
    glass_tint: 0.6,
    ice_gloss: 0.8,
    wet_gloss: 1.0,
    puddles: 0.8,
    water_reflection: 1.0,
    sway: 1.0,
    sway_speed: 1.3,
    fungus: 0x7cffc0,
    fungus_glow: 0.6,
    magic_glow: 0.8,
    lava_heat: 0.6,
    heat_flicker: 0.35,
    corruption: 1.0,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialUniform {
    hero_position: [f32; 4],
    hero_colour: [f32; 4],
    room_colour: [f32; 4],
    heat_colour: [f32; 4],
    fungus_colour: [f32; 4],
    hero_reach: f32,
    wall_height: f32,
    metal_shine: f32,
    metal_sharpness: f32,
    crystal_glints: f32,
    crystal_colours: f32,
    crystal_depth: f32,
    see_through: f32,
    glass_tint: f32,
    ice_gloss: f32,
    wet_gloss: f32,
    puddles: f32,
    water_reflection: f32,
    sway: f32,
    sway_speed: f32,
    fungus_glow: f32,
    magic_glow: f32,
    lava_heat: f32,
    heat_flicker: f32,
    corruption: f32,
}

fn colour_of(hex: Hex, strength: f32) -> [f32; 4] {
    let [r, g, b] = linear(hex);
    [r * strength, g * strength, b * strength, 1.0]
}

fn grid_size() -> u32 {
    u32::try_from(MEMORY_CELLS).unwrap_or(128)
}

fn grid_side() -> i32 {
    i32::try_from(MEMORY_CELLS).unwrap_or(128)
}

fn texel_of(x: i32, y: i32) -> usize {
    let side = grid_side();
    usize::try_from(y.rem_euclid(side) * side + x.rem_euclid(side)).unwrap_or(0) * 4
}

fn cell_distance(dx: i32, dy: i32) -> f32 {
    let float = |v: i32| i16::try_from(v).map(f32::from).unwrap_or(f32::MAX);
    float(dx).hypot(float(dy))
}

fn half_bits(values: &[f32]) -> Vec<u16> {
    values.iter().map(|v| f16::from_f32(*v).to_bits()).collect()
}

pub fn door_metal(colour: &[u8]) -> Vec<u8> {
    let metal = MaterialClass::Metal.texel_alpha();
    colour
        .chunks_exact(4)
        .flat_map(|p| {
            let (high, low) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
            let grey = high > 40 && high - low < 16;
            [128, 128, 255, if grey { metal } else { 0 }]
        })
        .collect()
}

pub fn world_source() -> String {
    with_common(&[include_str!("shaders/world.wgsl"), include_str!("shaders/materials.wgsl"), include_str!("shaders/materials_world.wgsl")].join("\n"))
}

pub fn water_source() -> String {
    [include_str!("shaders/liquids.wgsl"), include_str!("shaders/materials.wgsl"), include_str!("shaders/materials_water.wgsl")].join("\n")
}

pub struct MaterialLayer {
    pub layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    cells: wgpu::Texture,
    heat: wgpu::Texture,
    pub group: wgpu::BindGroup,
    module: wgpu::ShaderModule,
    world: HashMap<Material, wgpu::RenderPipeline>,
    samples: u32,
    cell_lights: u32,
    cell_values: Vec<f32>,
    heat_values: Vec<f32>,
    built_revision: u64,
    glowing: bool,
}

impl MaterialLayer {
    pub fn new(gpu: &Gpu) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("materials"),
            entries: &[uniform_entry(0, UNIFORM_SIZE, false), texture(1), texture(2)],
        });
        let uniform = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("materials"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let empty = vec![0u8; MEMORY_CELLS * MEMORY_CELLS * 8];
        let cells = gpu.texture_with("material cells", grid_size(), grid_size(), wgpu::TextureFormat::Rgba16Float, &empty);
        let heat = gpu.texture_with("material heat", grid_size(), grid_size(), wgpu::TextureFormat::Rgba16Float, &empty);
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("materials"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view(&cells)) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&view(&heat)) },
            ],
        });
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("world materials"), source: wgpu::ShaderSource::Wgsl(world_source().into()) });
        MaterialLayer {
            layout,
            uniform,
            cells,
            heat,
            group,
            module,
            world: HashMap::new(),
            samples: 0,
            cell_lights: 0,
            cell_values: vec![0.0; MEMORY_CELLS * MEMORY_CELLS * 4],
            heat_values: vec![0.0; MEMORY_CELLS * MEMORY_CELLS * 4],
            built_revision: u64::MAX,
            glowing: false,
        }
    }

    pub fn prepare(&mut self, gpu: &Gpu, samples: u32, cell_lights: u32) {
        if (self.samples, self.cell_lights) == (samples, cell_lights) {
            return;
        }
        self.world = WORLD_MATERIALS.iter().map(|m| (*m, gpu.world_pipeline_with(&self.module, Some(&self.layout), *m, samples, cell_lights))).collect();
        (self.samples, self.cell_lights) = (samples, cell_lights);
    }

    pub fn world_pipelines(&self) -> &HashMap<Material, wgpu::RenderPipeline> {
        &self.world
    }

    pub fn glowing(&self) -> bool {
        self.glowing
    }

    pub fn sync(&mut self, gpu: &Gpu, scene: &Scene, revision: u64, wall_colour: &mut dyn FnMut(TileId) -> Option<[f32; 3]>) {
        if revision == self.built_revision {
            return;
        }
        self.built_revision = revision;
        let mut cells = vec![0.0f32; MEMORY_CELLS * MEMORY_CELLS * 4];
        let mut heat = vec![0.0f32; MEMORY_CELLS * MEMORY_CELLS * 4];
        let mut lava = Vec::new();
        for c in scene.cells.values() {
            if c.visibility == Visibility::Unseen {
                continue;
            }
            if c.occluder {
                if let Some([r, g, b]) = c.wall_tile.and_then(&mut *wall_colour) {
                    let i = texel_of(c.x, c.y);
                    cells[i..i + 4].copy_from_slice(&[r, g, b, if c.flags.torch_wall { TORCH_WALL } else { PLAIN_WALL }]);
                }
            }
            if c.liquid.is_some_and(|l| l.kind == LiquidKind::Lava) {
                lava.push((c.x, c.y));
            }
        }
        for &(x, y) in &lava {
            for dy in -HEAT_CELLS..=HEAT_CELLS {
                for dx in -HEAT_CELLS..=HEAT_CELLS {
                    let warmth = (1.0 - cell_distance(dx, dy) / HEAT_REACH).max(0.0);
                    let i = texel_of(x + dx, y + dy);
                    heat[i] = heat[i].max(warmth);
                }
            }
        }
        self.glowing = !lava.is_empty();
        if cells != self.cell_values {
            gpu.write(&self.cells, grid_size(), grid_size(), bytemuck::cast_slice(&half_bits(&cells)));
            self.cell_values = cells;
        }
        if heat != self.heat_values {
            gpu.write(&self.heat, grid_size(), grid_size(), bytemuck::cast_slice(&half_bits(&heat)));
            self.heat_values = heat;
        }
    }

    pub fn configure(&self, gpu: &Gpu, lights: &IsoLights, hero: Option<(f32, f32)>, view: Mat4, wall_height: f32) {
        let m = &lights.materials;
        let hero_view = hero.map_or(Vec3::splat(f32::MAX), |(x, y)| view.transform_point3(Vec3::new(x + 0.5, HERO_HEIGHT, y + 0.5)));
        let u = MaterialUniform {
            hero_position: [hero_view.x, hero_view.y, hero_view.z, 1.0],
            hero_colour: colour_of(m.hero_light, m.hero_strength),
            room_colour: colour_of(m.room, 1.0),
            heat_colour: colour_of(lights.lava.molten, 1.0),
            fungus_colour: colour_of(m.fungus, 1.0),
            hero_reach: m.hero_reach,
            wall_height,
            metal_shine: m.metal_shine,
            metal_sharpness: m.metal_sharpness,
            crystal_glints: m.crystal_glints,
            crystal_colours: m.crystal_colours,
            crystal_depth: m.crystal_depth,
            see_through: m.see_through,
            glass_tint: m.glass_tint,
            ice_gloss: m.ice_gloss,
            wet_gloss: m.wet_gloss,
            puddles: m.puddles,
            water_reflection: m.water_reflection,
            sway: m.sway,
            sway_speed: m.sway_speed,
            fungus_glow: m.fungus_glow,
            magic_glow: m.magic_glow,
            lava_heat: m.lava_heat,
            heat_flicker: m.heat_flicker,
            corruption: m.corruption,
        };
        gpu.queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::decode_label;

    #[test]
    fn tiles_take_the_material_their_label_names() {
        // given
        let metal_wall = decode_label(&[10, 1, 1, 2, 2, 0, 0, 0, 0, 0]);
        let crystal_wall = decode_label(&[12, 0, 1, 0, 3, 0, 0, 1, 0, 0]);
        let glass_wall = decode_label(&[15, 0, 1, 0, 2, 0, 0, 0, 1, 0]);
        let tree = decode_label(&[28, 1, 1, 0, 0, 0, 0, 0, 0, 0]);
        let wood = decode_label(&[9, 2, 1, 4, 1, 0, 0, 0, 0, 0]);

        // when
        let classes: Vec<MaterialClass> = [metal_wall, crystal_wall, glass_wall, tree, wood].iter().map(MaterialClass::of).collect();

        // then
        assert_eq!(classes, [MaterialClass::Metal, MaterialClass::Crystal, MaterialClass::Glass, MaterialClass::Plant, MaterialClass::Plain]);
        assert!(classes.iter().all(|c| MaterialClass::of_texel_alpha(c.texel_alpha()) == *c));
    }
}
