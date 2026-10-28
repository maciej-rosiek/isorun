use crate::actors::ActorEnv;
use crate::gpu::Gpu;
use crate::lights::{linear, IsoLights};
use crate::memory::MEMORY_CELLS;
use crate::torch_light::{attenuation, clear};
use bytemuck::{Pod, Zeroable};
use half::f16;
use isorun_scene::{BillboardKind, CellKind, LiquidKind, Rgba8, Scene, SceneCell, SpriteEffect, TileId, Visibility};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

const GI_FLAG: u32 = 1;
const SIGHT_FLAG: u32 = 2;
const OPEN_WEIGHT: f32 = 1.0;
const WALL_WEIGHT: f32 = 0.004;
const WALL_TOP_SHARE: f32 = 0.3;
const HERO_HEIGHT: f32 = 1.6;
const LEAST_SIGHT_REACH: f32 = 2.5;
const LIGHT_DECAY: f32 = 1.0;
const SPREAD_ROUNDS: usize = 6;
const SPREAD_KEEP: f32 = 0.8;
const NEIGHBOURS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
const LAVA_LIGHT: f32 = 0.35;
const TORCH_WALL_LIGHT: f32 = 2.0;
const GLOWING_TILE_LIGHT: f32 = 0.8;
const GLOWING_ACTOR_LIGHT: f32 = 1.5;
const BURNING_ACTOR_LIGHT: f32 = 2.0;
const HERO_LIGHT: f32 = 8.0;
const FLASH_FULL_ALPHA: f32 = 100.0;
const HALO_LIGHT: f32 = 6.0;
const GI_RISE_SECONDS: f64 = 0.05;
const GI_FALL_SECONDS: f64 = 0.35;
const SIGHT_SECONDS: f64 = 0.12;
const SETTLED: f64 = 0.002;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellLights {
    pub gi: bool,
    pub sight: bool,
}

impl CellLights {
    pub fn flags(self) -> u32 {
        u32::from(self.gi) * GI_FLAG | u32::from(self.sight) * SIGHT_FLAG
    }

    pub fn any(self) -> bool {
        self.gi || self.sight
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq)]
struct Emitter {
    x: i32,
    y: i32,
    colour: Rgb,
    shown: bool,
}

impl Emitter {
    fn key(&self) -> (i32, i32, [u32; 3], bool) {
        (self.x, self.y, self.colour.map(f32::to_bits), self.shown)
    }
}

fn hash_of<T: Hash>(v: &T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

fn cell_f32(v: i32) -> f32 {
    f32::from(i16::try_from(v).unwrap_or(i16::MAX))
}

fn whole_cells(reach: f32) -> i32 {
    (0i16..i16::MAX).find(|r| f32::from(*r) >= reach).map_or(0, i32::from)
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

fn added(a: Rgb, b: Rgb) -> Rgb {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn vivid(c: Rgb) -> Rgb {
    let most = c[0].max(c[1]).max(c[2]);
    if most <= 0.0 {
        return [0.0; 3];
    }
    scaled(c, 1.0 / most)
}

fn flash_colour(flash: Rgba8, strength: f32) -> Rgb {
    let hex = u32::from(flash.r) << 16 | u32::from(flash.g) << 8 | u32::from(flash.b);
    scaled(linear(hex), strength * f32::from(flash.a) / FLASH_FULL_ALPHA)
}

fn is_open(c: &SceneCell) -> bool {
    !c.occluder && c.kind != CellKind::Unknown
}

fn wall_tile(c: &SceneCell) -> Option<TileId> {
    c.face_tiles.and_then(|f| f.s.or(f.e).or(f.n).or(f.w)).or(c.wall_tile)
}

struct Grid {
    x0: i32,
    y0: i32,
    width: i32,
    height: i32,
    open: Vec<bool>,
    albedo: Vec<Rgb>,
}

impl Grid {
    fn of(scene: &Scene, albedo_of: &mut dyn FnMut(&SceneCell) -> Rgb) -> Option<Grid> {
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for c in scene.cells.values() {
            (x0, y0, x1, y1) = (x0.min(c.x), y0.min(c.y), x1.max(c.x), y1.max(c.y));
        }
        if x0 > x1 {
            return None;
        }
        let (x0, y0, width, height) = (x0 - 1, y0 - 1, x1 - x0 + 3, y1 - y0 + 3);
        let size = usize::try_from(width * height).ok()?;
        let mut grid = Grid { x0, y0, width, height, open: vec![false; size], albedo: vec![[0.0; 3]; size] };
        for c in scene.cells.values() {
            let Some(i) = grid.index(c.x, c.y) else { continue };
            grid.open[i] = is_open(c);
            grid.albedo[i] = albedo_of(c);
        }
        Some(grid)
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let (dx, dy) = (x - self.x0, y - self.y0);
        if dx < 0 || dy < 0 || dx >= self.width || dy >= self.height {
            return None;
        }
        usize::try_from(dy * self.width + dx).ok()
    }

    fn blocked(&self, x: i32, y: i32) -> bool {
        self.index(x, y).is_none_or(|i| !self.open[i])
    }

    fn cells(&self) -> impl Iterator<Item = (i32, i32, usize)> + '_ {
        (self.y0..self.y0 + self.height).flat_map(move |y| (self.x0..self.x0 + self.width).filter_map(move |x| self.index(x, y).map(|i| (x, y, i))))
    }

    fn key(&self) -> u64 {
        hash_of(&(self.x0, self.y0, self.width, self.height, &self.open, self.albedo.iter().map(|a| a.map(f32::to_bits)).collect::<Vec<_>>()))
    }
}

fn facing_cell(grid: &Grid, from: (i32, i32), x: i32, y: i32) -> Option<usize> {
    let away = |(nx, ny): (i32, i32)| (nx - from.0).pow(2) + (ny - from.1).pow(2);
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .map(|(dx, dy)| (x + dx, y + dy))
        .filter(|(nx, ny)| !grid.blocked(*nx, *ny))
        .min_by_key(|n| away(*n))
        .and_then(|(nx, ny)| grid.index(nx, ny))
}

fn lit_by(grid: &Grid, emitters: &[Emitter], reach: f32) -> (Vec<Rgb>, Vec<Rgb>) {
    let size = grid.open.len();
    let (mut shown, mut given) = (vec![[0.0; 3]; size], vec![[0.0; 3]; size]);
    let cells = whole_cells(reach);
    let centre = |x: i32, y: i32| (cell_f32(x) + 0.5, cell_f32(y) + 0.5);
    for e in emitters {
        for dy in -cells..=cells {
            for dx in -cells..=cells {
                let (x, y) = (e.x + dx, e.y + dy);
                let Some(i) = grid.index(x, y) else { continue };
                let d = cell_f32(dx).hypot(cell_f32(dy));
                if d > reach || !clear(|x, y| grid.blocked(x, y), centre(e.x, e.y), centre(x, y)) {
                    continue;
                }
                let light = scaled(e.colour, attenuation(d, reach, LIGHT_DECAY));
                if e.shown {
                    shown[i] = added(shown[i], light);
                }
                let receiver = if grid.open[i] { Some(i) } else { facing_cell(grid, (e.x, e.y), x, y) };
                if let Some(r) = receiver {
                    let a = grid.albedo[i];
                    given[r] = added(given[r], [light[0] * a[0], light[1] * a[1], light[2] * a[2]]);
                }
            }
        }
    }
    (shown, given)
}

fn bounced(grid: &Grid, given: &[Rgb]) -> Vec<Rgb> {
    let mut field = given.to_vec();
    let share = SPREAD_KEEP / 8.0;
    for _ in 0..SPREAD_ROUNDS {
        let mut next = given.to_vec();
        for (x, y, i) in grid.cells() {
            if !grid.open[i] {
                continue;
            }
            let around = NEIGHBOURS.iter().filter_map(|(dx, dy)| grid.index(x + dx, y + dy)).fold([0.0; 3], |sum, n| added(sum, field[n]));
            next[i] = added(next[i], scaled(around, share));
        }
        field = next;
    }
    field
}

fn light_field(grid: &Grid, emitters: &[Emitter], reach: f32, bounce: f32) -> Vec<Rgb> {
    if emitters.is_empty() {
        return vec![[0.0; 3]; grid.open.len()];
    }
    let (shown, given) = lit_by(grid, emitters, reach);
    let spread = bounced(grid, &given);
    shown.iter().zip(&spread).map(|(s, b)| added(*s, scaled(*b, bounce))).collect()
}

fn gi_texels(grid: &Grid, parts: [&[Rgb]; 2]) -> Vec<[f32; 4]> {
    let mut texels = vec![[0.0; 4]; MEMORY_CELLS * MEMORY_CELLS];
    for (x, y, i) in grid.cells() {
        let (weight, share) = if grid.open[i] { (OPEN_WEIGHT, 1.0) } else { (WALL_WEIGHT, WALL_TOP_SHARE) };
        let [r, g, b] = scaled(added(parts[0][i], parts[1][i]), weight * share);
        texels[texel_of(x, y)] = [r, g, b, weight];
    }
    texels
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

struct Emitters {
    lasting: Vec<Emitter>,
    passing: Vec<Emitter>,
}

fn emitters(scene: &Scene, lights: &IsoLights, on: CellLights, glow_of: &mut dyn FnMut(TileId) -> Option<Rgb>) -> Emitters {
    let gi = lights.gi;
    let mut lasting = Vec::new();
    let mut passing = Vec::new();
    if gi.hero > 0.0 {
        let (x, y) = scene.player;
        passing.push(Emitter { x, y, colour: scaled(linear(lights.sight.colour), HERO_LIGHT * gi.hero), shown: !on.sight });
    }
    let at = |c: &SceneCell, colour: Rgb, shown: bool| Emitter { x: c.x, y: c.y, colour, shown };
    for c in scene.cells.values() {
        if c.visibility == Visibility::Unseen {
            continue;
        }
        if c.liquid.is_some_and(|l| l.kind == LiquidKind::Lava) {
            lasting.push(at(c, scaled(linear(gi.lava), LAVA_LIGHT), true));
        }
        if c.flags.torch_wall {
            lasting.push(at(c, scaled(linear(lights.torch.colour), TORCH_WALL_LIGHT), false));
        }
        let terrain = if is_open(c) { Some(c.floor_tile) } else { wall_tile(c) };
        for tile in [c.feature_tile, terrain].into_iter().flatten() {
            if let Some(colour) = glow_of(tile) {
                lasting.push(at(c, scaled(colour, GLOWING_TILE_LIGHT), true));
            }
        }
        if let Some(flash) = c.flash {
            passing.push(at(c, flash_colour(flash, gi.flash), true));
        }
        if let Some(colour) = c.cloud.and_then(|t| glow_of(t)) {
            passing.push(at(c, colour, true));
        }
    }
    for b in &scene.billboards {
        let actor = matches!(b.kind, BillboardKind::Monster | BillboardKind::Player);
        let mut colour = glow_of(b.tile);
        if actor && b.effects.contains(&SpriteEffect::Glowing) {
            colour = Some(added(colour.unwrap_or([0.0; 3]), scaled(linear(lights.sight.halo), GLOWING_ACTOR_LIGHT)));
        }
        if actor && b.effects.contains(&SpriteEffect::Burning) {
            colour = Some(added(colour.unwrap_or([0.0; 3]), scaled(linear(gi.fire), BURNING_ACTOR_LIGHT)));
        }
        if let Some(colour) = colour {
            passing.push(Emitter { x: b.x, y: b.y, colour, shown: true });
        }
    }
    for list in [&mut lasting, &mut passing] {
        list.sort_by(|a, b| (a.y, a.x).cmp(&(b.y, b.x)).then(a.colour[0].total_cmp(&b.colour[0])));
    }
    Emitters { lasting, passing }
}

fn albedo_of(env: &mut ActorEnv, c: &SceneCell) -> Rgb {
    let tile = if is_open(c) { Some(c.floor_tile) } else { wall_tile(c) };
    tile.and_then(|t| env.tiles.tile(t)).map_or([0.0; 3], |rect| env.pages.average_colour(env.tiles, &rect))
}

fn glow_of(glows: &mut HashMap<TileId, Option<Rgb>>, env: &mut ActorEnv, tile: TileId) -> Option<Rgb> {
    if let Some(known) = glows.get(&tile) {
        return *known;
    }
    let found = env.tiles.tile(tile).and_then(|rect| {
        let placed = env.pages.for_tile(&env.gpu.device, &env.gpu.page_layout, &env.gpu.page_sampler, env.tiles, &rect)?;
        let glow = placed.label.map_or(0, |l| l.glow);
        (glow > 0).then(|| scaled(vivid(env.pages.average_colour(env.tiles, &rect)), f32::from(glow)))
    });
    glows.insert(tile, found);
    found
}

struct EasedField {
    texture: wgpu::Texture,
    target: Vec<[f64; 4]>,
    shown: Vec<[f64; 4]>,
    rise_seconds: f64,
    fall_seconds: f64,
    settled: bool,
    dirty: bool,
}

impl EasedField {
    fn new(gpu: &Gpu, label: &str, rise_seconds: f64, fall_seconds: f64) -> Self {
        let n = u32::try_from(MEMORY_CELLS).unwrap_or(1);
        let empty = vec![[0.0; 4]; MEMORY_CELLS * MEMORY_CELLS];
        let texture = gpu.texture_with(label, n, n, wgpu::TextureFormat::Rgba16Float, bytemuck::cast_slice(&half_texels(&empty)));
        EasedField { texture, target: empty.clone(), shown: empty, rise_seconds, fall_seconds, settled: true, dirty: false }
    }

    fn retarget(&mut self, target: &[[f32; 4]]) {
        let target: Vec<[f64; 4]> = target.iter().map(|t| t.map(f64::from)).collect();
        if target != self.target {
            self.target = target;
            self.settled = false;
        }
    }

    fn snap(&mut self) {
        self.shown.clone_from(&self.target);
        self.settled = true;
        self.dirty = true;
    }

    fn advance(&mut self, dt: f64) {
        if self.settled {
            return;
        }
        let (rise, fall) = (1.0 - (-dt / self.rise_seconds).exp(), 1.0 - (-dt / self.fall_seconds).exp());
        let mut moving = false;
        for (shown, target) in self.shown.iter_mut().zip(&self.target) {
            for (s, t) in shown.iter_mut().zip(target) {
                let gap = t - *s;
                if gap.abs() <= SETTLED {
                    *s = *t;
                } else {
                    *s += gap * if gap > 0.0 { rise } else { fall };
                    moving = true;
                }
            }
        }
        self.settled = !moving;
        self.dirty = true;
    }

    fn upload(&mut self, gpu: &Gpu) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let n = u32::try_from(MEMORY_CELLS).unwrap_or(1);
        gpu.write(&self.texture, n, n, bytemuck::cast_slice(&half_texels(&self.shown)));
    }
}

fn half_texels(texels: &[[f64; 4]]) -> Vec<u16> {
    texels.iter().flat_map(|t| t.map(|c| f16::from_f64(c).to_bits())).collect()
}

pub struct CellLight {
    pub buffer: wgpu::Buffer,
    gi: EasedField,
    sight: EasedField,
    updated_for: Option<(u64, u64)>,
    lasting_key: u64,
    lasting: Vec<Rgb>,
    passing_key: u64,
    passing: Vec<Rgb>,
    glows: HashMap<TileId, Option<Rgb>>,
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
        CellLight {
            buffer,
            gi: EasedField::new(gpu, "cell gi", GI_RISE_SECONDS, GI_FALL_SECONDS),
            sight: EasedField::new(gpu, "cell sight", SIGHT_SECONDS, SIGHT_SECONDS),
            updated_for: None,
            lasting_key: 0,
            lasting: Vec::new(),
            passing_key: 0,
            passing: Vec::new(),
            glows: HashMap::new(),
            sight_reach: LEAST_SIGHT_REACH,
        }
    }

    pub fn gi_texture(&self) -> &wgpu::Texture {
        &self.gi.texture
    }

    pub fn sight_texture(&self) -> &wgpu::Texture {
        &self.sight.texture
    }

    pub fn easing(&self) -> bool {
        !self.gi.settled || !self.sight.settled
    }

    pub fn snap(&mut self) {
        self.gi.snap();
        self.sight.snap();
    }

    pub fn update(&mut self, env: &mut ActorEnv, scene: &Scene, revision: u64, lights: &IsoLights, on: CellLights) {
        let gi = lights.gi;
        let settings = hash_of(&(
            on,
            [gi.reach_cells, gi.bounce, gi.flash, gi.hero].map(f32::to_bits),
            gi.lava,
            gi.fire,
            lights.torch.colour,
            lights.sight.halo,
            lights.sight.colour,
        ));
        if self.updated_for == Some((revision, settings)) {
            return;
        }
        self.updated_for = Some((revision, settings));
        if on.sight {
            let (texels, reach) = sight_texels(scene);
            self.sight_reach = reach;
            self.sight.retarget(&texels);
        }
        if on.gi {
            self.bake_gi(env, scene, lights, on);
        }
    }

    fn bake_gi(&mut self, env: &mut ActorEnv, scene: &Scene, lights: &IsoLights, on: CellLights) {
        let Some(grid) = Grid::of(scene, &mut |c| albedo_of(env, c)) else { return };
        let glows = &mut self.glows;
        let found = emitters(scene, lights, on, &mut |tile| glow_of(glows, env, tile));
        let (reach, bounce) = (lights.gi.reach_cells, lights.gi.bounce);
        let grid_key = hash_of(&(grid.key(), reach.to_bits(), bounce.to_bits()));
        let lasting_key = hash_of(&(grid_key, found.lasting.iter().map(Emitter::key).collect::<Vec<_>>()));
        if lasting_key != self.lasting_key {
            self.lasting = light_field(&grid, &found.lasting, reach, bounce);
            self.lasting_key = lasting_key;
        }
        let passing_key = hash_of(&(grid_key, found.passing.iter().map(Emitter::key).collect::<Vec<_>>()));
        if passing_key != self.passing_key {
            self.passing = light_field(&grid, &found.passing, reach, bounce);
            self.passing_key = passing_key;
        }
        self.gi.retarget(&gi_texels(&grid, [&self.lasting, &self.passing]));
    }

    pub fn advance(&mut self, gpu: &Gpu, dt: f64) {
        self.gi.advance(dt);
        self.sight.advance(dt);
        self.gi.upload(gpu);
        self.sight.upload(gpu);
    }

    pub fn uniform(&self, lights: &IsoLights, hero: Option<(f32, f32)>) -> CellLightUniform {
        let (gi, sight) = (lights.gi, lights.sight);
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
            fill: [gi.strength, gi.fill_kept, sight.fill_kept, sight.flicker],
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
    use isorun_scene::{CellKind, Liquid, Scene, SceneCell};

    fn cell(x: i32, y: i32, kind: CellKind, visibility: Visibility) -> SceneCell {
        SceneCell { x, y, kind, visibility, occluder: kind == CellKind::Wall, ..SceneCell::default() }
    }

    fn row(cells: &str, visibility: impl Fn(i32) -> Visibility) -> Scene {
        let mut scene = Scene::default();
        for (x, c) in (0i32..).zip(cells.chars()) {
            let mut made = cell(x, 0, if c == '#' { CellKind::Wall } else { CellKind::Floor }, visibility(x));
            if c == 'L' {
                made.liquid = Some(Liquid { kind: LiquidKind::Lava, deep: true });
            }
            scene.cells.insert(isorun_scene::cell_key(x, 0), made);
        }
        scene
    }

    #[test]
    fn lava_lights_the_floor_it_reaches_and_the_walls_bounce_their_colour() {
        // given
        let scene = row("#L..#.", |_| Visibility::Visible);
        let lights = crate::lights::diorama();
        let green_walls = |c: &SceneCell| if c.occluder { [0.1, 0.8, 0.1] } else { [0.3, 0.3, 0.3] };
        let grid = Grid::of(&scene, &mut |c| green_walls(c)).expect("a grid");
        let found = emitters(&scene, &lights, CellLights { gi: true, sight: false }, &mut |_| None);

        // when
        let field = light_field(&grid, &found.lasting, lights.gi.reach_cells, lights.gi.bounce);

        // then
        let at = |x: i32| field[grid.index(x, 0).expect("in the grid")];
        let (beside, near_wall, lava) = (at(2), at(3), linear(lights.gi.lava));
        assert!(beside[0] > beside[2], "lava light is warm: {beside:?}");
        assert!(near_wall[1] / near_wall[0] > lava[1] / lava[0], "the green wall gives back green: {near_wall:?}");
        assert_eq!(at(5), [0.0; 3], "no light past the wall");
    }

    #[test]
    fn the_hero_lights_the_room_and_its_walls_bounce_their_colour() {
        // given
        let mut scene = row("#...#.", |_| Visibility::Visible);
        scene.player = (1, 0);
        let lights = crate::lights::diorama();
        let green_walls = |c: &SceneCell| if c.occluder { [0.1, 0.8, 0.1] } else { [0.3, 0.3, 0.3] };
        let grid = Grid::of(&scene, &mut |c| green_walls(c)).expect("a grid");
        let found = emitters(&scene, &lights, CellLights { gi: true, sight: false }, &mut |_| None);

        // when
        let field = light_field(&grid, &found.passing, lights.gi.reach_cells, lights.gi.bounce);

        // then
        let at = |x: i32| field[grid.index(x, 0).expect("in the grid")];
        let hero = linear(lights.sight.colour);
        assert!(at(2)[0] > 0.0, "the hero's light reaches the floor: {:?}", at(2));
        assert!(at(3)[1] / at(3)[0] > hero[1] / hero[0], "the green wall gives back green: {:?}", at(3));
        assert_eq!(at(5), [0.0; 3], "no light past the wall");
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
