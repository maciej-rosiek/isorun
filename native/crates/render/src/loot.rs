//! What a kill leaves, as Diablo shows it: an item a monster drops as it dies flies up out of it,
//! turns over and lands on its cell, and an epic, legendary or unique item on the floor in sight
//! stands in a column of light of its colour, with a glow on the floor and a light around it.

use crate::actors::{sprite_key, ActorEnv, SpriteVisual, UprightPose};
use crate::gpu::{Blend, Gpu};
use crate::lights::linear;
use crate::particles::{Instance, ParticleBatch, ParticleShared};
use crate::renderer::DrawItem;
use glam::{EulerRot, Mat4, Quat, Vec3, Vec4};
use isorun_scene::{Billboard, BillboardKind, ItemRarity, Scene, TileId, Visibility};
use std::collections::{HashMap, HashSet};

/// The loot colours: WoW's and Diablo Immortal's epic purple, the legendary orange of Diablo III
/// and IV, and Diablo IV's unique gold; each with how strong its light shines, the purple the
/// most as the eye sees it the darkest.
const RARITIES: [(ItemRarity, u32, f32); 3] = [(ItemRarity::Epic, 0xa335ee, 2.4), (ItemRarity::Legendary, 0xff8000, 1.4), (ItemRarity::Unique, 0xc7b377, 1.2)];

/// A death and the item that shows on its cell after it go together this long, on the actors' clock.
const DEATH_REMEMBERED: f64 = 1.5;
/// From the blow that kills to the item leaving the body.
const POP_DELAY: f64 = 0.12;
const FLIGHT_SECONDS: f64 = 0.55;
const FLIGHT_HEIGHT: f32 = 0.9;
const FLIGHT_START: f32 = 0.3;
const FLIGHT_TURNS: f32 = 2.0;
const FLIGHT_DRIFT: f32 = 0.25;
const BOUNCE_SECONDS: f64 = 0.18;
const BOUNCE_HEIGHT: f32 = 0.08;
/// The middle of an item's art, which it turns about in the air, in cells up its tile.
const TURN_PIVOT: f32 = 0.5;

const MARK_FADE_SECONDS: f64 = 0.4;
const BEAM_WIDTH: f32 = 0.42;
const BEAM_HEIGHT: f32 = 2.6;
const HALO_SIZE: f32 = 1.1;
const HALO_HEIGHT: f32 = 0.006;
const LIGHT_HEIGHT: f32 = 0.35;
pub const LIGHT_REACH: f32 = 2.2;
const LIGHT_INTENSITY: f32 = 3.0;

fn rarity_index(rarity: ItemRarity) -> usize {
    RARITIES.iter().position(|r| r.0 == rarity).unwrap_or(0)
}

/// A seed from 0 to 1 by the cell, so marks side by side do not shimmer together.
fn cell_seed(cell: (i32, i32)) -> f32 {
    let h = (cell.0 as u32).wrapping_mul(374761393) ^ (cell.1 as u32).wrapping_mul(668265263);
    (h % 1000) as f32 / 1000.0
}

/// An item landed on the floor: where, and how good it looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landing {
    pub cell: (i32, i32),
    pub rarity: Option<ItemRarity>,
}

struct Flight {
    billboard: Billboard,
    visual: SpriteVisual,
    start: f64,
    /// Which way it turns over, and where it drifts to and back from.
    turn: f32,
    drift: (f32, f32),
    landed: bool,
    model: Mat4,
}

impl Flight {
    fn cell(&self) -> (i32, i32) {
        (self.billboard.x, self.billboard.y)
    }

    fn done(&self, now: f64) -> bool {
        now >= self.start + FLIGHT_SECONDS + BOUNCE_SECONDS
    }

    fn place(&mut self, now: f64, yaw: f32, pose: UprightPose, ground: f32) {
        let age = now - self.start;
        let (height, turned, away) = if age < FLIGHT_SECONDS {
            let t = (age / FLIGHT_SECONDS) as f32;
            (
                FLIGHT_START * (1.0 - t) + 4.0 * FLIGHT_HEIGHT * t * (1.0 - t),
                self.turn * std::f32::consts::TAU * FLIGHT_TURNS * t,
                (std::f32::consts::PI * t).sin(),
            )
        } else {
            let u = ((age - FLIGHT_SECONDS) / BOUNCE_SECONDS).min(1.0) as f32;
            (4.0 * BOUNCE_HEIGHT * u * (1.0 - u), 0.0, 0.0)
        };
        let position = Vec3::new(self.billboard.x as f32 + 0.5 + self.drift.0 * away, ground + height, self.billboard.y as f32 + 0.5 + self.drift.1 * away);
        let pivot = Vec3::new(0.0, TURN_PIVOT * pose.stretch, 0.0);
        let facing = Quat::from_euler(EulerRot::YXZ, -yaw, -pose.lean, 0.0);
        self.model = Mat4::from_translation(position)
            * Mat4::from_quat(facing)
            * Mat4::from_translation(pivot)
            * Mat4::from_rotation_z(turned)
            * Mat4::from_translation(-pivot)
            * Mat4::from_scale(Vec3::new(1.0, pose.stretch, 1.0));
    }
}

struct Mark {
    cell: (i32, i32),
    rarity: ItemRarity,
    since: f64,
}

pub struct LootLayer {
    shared: ParticleShared,
    beams: Vec<ParticleBatch>,
    halos: Vec<ParticleBatch>,
    /// The top item last seen on each cell, kept while a monster stands over it.
    known: HashMap<(i32, i32), TileId>,
    deaths: Vec<((i32, i32), f64)>,
    flights: Vec<Flight>,
    /// The good items on the floor in sight.
    worth: Vec<((i32, i32), ItemRarity)>,
    marks: Vec<Mark>,
    landings: Vec<Landing>,
    clock: f64,
}

impl LootLayer {
    pub fn new(gpu: &Gpu) -> Self {
        let shared = ParticleShared::new(gpu);
        let batches = |fs: &'static str| -> Vec<ParticleBatch> {
            RARITIES
                .iter()
                .map(|(_, colour, strength)| {
                    let batch = ParticleBatch::new(gpu, &shared, fs, Blend::Additive, false);
                    batch.colour(gpu, *colour, *strength);
                    batch
                })
                .collect()
        };
        let (beams, halos) = (batches("fs_beam"), batches("fs_halo"));
        LootLayer {
            shared,
            beams,
            halos,
            known: HashMap::new(),
            deaths: Vec::new(),
            flights: Vec::new(),
            worth: Vec::new(),
            marks: Vec::new(),
            landings: Vec::new(),
            clock: 0.0,
        }
    }

    pub fn prepare(&mut self, gpu: &Gpu, samples: u32) {
        for batch in self.beams.iter_mut().chain(self.halos.iter_mut()) {
            batch.prepare(gpu, &self.shared, samples);
        }
    }

    /// A monster died on `cell`, its blow landing at `at` on the actors' clock.
    pub fn death(&mut self, cell: (i32, i32), at: f64) {
        self.deaths.push((cell, at));
    }

    /// A new scene: an item new on the cell of a death flies out of it, and the good items in
    /// sight are marked.
    pub fn sync(&mut self, scene: &Scene, env: &mut ActorEnv) {
        let covered: HashSet<(i32, i32)> =
            scene.billboards.iter().filter(|b| matches!(b.kind, BillboardKind::Monster | BillboardKind::Player)).map(|b| (b.x, b.y)).collect();
        let items: Vec<&Billboard> = scene
            .billboards
            .iter()
            .filter(|b| b.kind == BillboardKind::Item && scene.cell(b.x, b.y).is_some_and(|c| c.visibility != Visibility::Unseen))
            .collect();
        for b in &items {
            let cell = (b.x, b.y);
            if self.known.insert(cell, b.tile) == Some(b.tile) || b.lying || b.scenery {
                continue;
            }
            let Some(i) = self.deaths.iter().position(|d| d.0 == cell) else { continue };
            let (_, at) = self.deaths.remove(i);
            self.flights.retain(|f| f.cell() != cell);
            let seed = cell_seed(cell);
            let angle = seed * std::f32::consts::TAU;
            let visual = SpriteVisual::new(b, env, sprite_key(b), false, 0.0);
            self.flights.push(Flight {
                billboard: (*b).clone(),
                visual,
                start: at + POP_DELAY,
                turn: if seed < 0.5 { 1.0 } else { -1.0 },
                drift: (angle.cos() * FLIGHT_DRIFT, angle.sin() * FLIGHT_DRIFT),
                landed: false,
                model: Mat4::IDENTITY,
            });
        }
        let shown: HashSet<(i32, i32)> = items.iter().map(|b| (b.x, b.y)).collect();
        self.known.retain(|cell, _| shown.contains(cell) || covered.contains(cell));
        self.worth = items
            .iter()
            .filter(|b| scene.cell(b.x, b.y).is_some_and(|c| c.visibility == Visibility::Visible))
            .filter_map(|b| b.rarity.map(|r| ((b.x, b.y), r)))
            .collect();
        self.mark(self.clock);
    }

    fn mark(&mut self, now: f64) {
        let flying: HashSet<(i32, i32)> = self.flights.iter().map(Flight::cell).collect();
        let old = std::mem::take(&mut self.marks);
        self.marks = self
            .worth
            .iter()
            .filter(|(cell, _)| !flying.contains(cell))
            .map(|&(cell, rarity)| Mark { cell, rarity, since: old.iter().find(|m| m.cell == cell && m.rarity == rarity).map_or(now, |m| m.since) })
            .collect();
    }

    /// Moves the items in the air on to `now`: the ones that touch the floor are landings, the
    /// ones that came to rest give their cell back to the level's geometry.
    pub fn advance(&mut self, now: f64, yaw: f32, pose: UprightPose, ground_at: &dyn Fn(i32, i32) -> f32) {
        self.clock = now;
        self.deaths.retain(|d| now - d.1 < DEATH_REMEMBERED);
        for f in &mut self.flights {
            if !f.landed && now >= f.start + FLIGHT_SECONDS {
                f.landed = true;
                self.landings.push(Landing { cell: f.cell(), rarity: f.billboard.rarity });
            }
            let cell = f.cell();
            f.place(now, yaw, pose, ground_at(cell.0, cell.1));
        }
        let before = self.flights.len();
        self.flights.retain(|f| !f.done(now));
        if self.flights.len() != before {
            self.mark(now);
        }
    }

    /// The cells whose item is in the air or about to leave a body: the level's geometry leaves it out.
    pub fn hidden(&self) -> Vec<(i32, i32)> {
        let mut cells: Vec<(i32, i32)> = self.flights.iter().map(Flight::cell).collect();
        cells.sort_unstable();
        cells
    }

    /// Something flies, or a mark still fades in.
    pub fn animating(&self) -> bool {
        !self.flights.is_empty() || self.marks.iter().any(|m| self.clock - m.since < MARK_FADE_SECONDS)
    }

    pub fn take_landings(&mut self) -> Vec<Landing> {
        std::mem::take(&mut self.landings)
    }

    /// The beams, each turned about its upright to face the camera and stretched up as the items
    /// are, and the glows on the floor.
    pub fn update(&mut self, gpu: &Gpu, right: Vec3, stretch: f32, ground_at: &dyn Fn(i32, i32) -> f32) {
        let across = Vec3::new(right.x, 0.0, right.z).normalize_or(Vec3::X);
        let facing = across.cross(Vec3::Y);
        let mut beams: Vec<Vec<Instance>> = vec![Vec::new(); RARITIES.len()];
        let mut halos: Vec<Vec<Instance>> = vec![Vec::new(); RARITIES.len()];
        for m in &self.marks {
            let fade = ((self.clock - m.since) / MARK_FADE_SECONDS).clamp(0.0, 1.0) as f32;
            let seed = cell_seed(m.cell);
            let ground = ground_at(m.cell.0, m.cell.1);
            let (x, z) = (m.cell.0 as f32 + 0.5, m.cell.1 as f32 + 0.5);
            let height = BEAM_HEIGHT * stretch * fade;
            let beam = Mat4::from_cols(
                (across * BEAM_WIDTH).extend(0.0),
                (Vec3::Y * height).extend(0.0),
                facing.extend(0.0),
                Vec4::new(x, ground + height / 2.0, z, 1.0),
            );
            let halo = Mat4::from_scale_rotation_translation(
                Vec3::splat(HALO_SIZE),
                Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                Vec3::new(x, ground + HALO_HEIGHT, z),
            );
            let i = rarity_index(m.rarity);
            beams[i].push(Instance { model: beam.to_cols_array_2d(), extra: [fade, seed, 0.0, 0.0] });
            halos[i].push(Instance { model: halo.to_cols_array_2d(), extra: [fade, seed, 0.0, 0.0] });
        }
        for (batch, instances) in self.beams.iter_mut().zip(&beams) {
            batch.set(gpu, instances);
        }
        for (batch, instances) in self.halos.iter_mut().zip(&halos) {
            batch.set(gpu, instances);
        }
    }

    /// The lights of the marks nearest `centre`, at most `count`: where, colour and strength.
    pub fn lights(&self, centre: (f32, f32), count: usize, ground_at: &dyn Fn(i32, i32) -> f32) -> Vec<(Vec3, [f32; 3], f32)> {
        let away = |m: &Mark| (m.cell.0 as f32 + 0.5 - centre.0).powi(2) + (m.cell.1 as f32 + 0.5 - centre.1).powi(2);
        let mut near: Vec<&Mark> = self.marks.iter().collect();
        near.sort_by(|a, b| away(a).total_cmp(&away(b)));
        near.into_iter()
            .take(count)
            .map(|m| {
                let (_, colour, strength) = RARITIES[rarity_index(m.rarity)];
                let fade = ((self.clock - m.since) / MARK_FADE_SECONDS).clamp(0.0, 1.0) as f32;
                let at = Vec3::new(m.cell.0 as f32 + 0.5, ground_at(m.cell.0, m.cell.1) + LIGHT_HEIGHT, m.cell.1 as f32 + 0.5);
                (at, linear(colour), strength * LIGHT_INTENSITY * fade)
            })
            .collect()
    }

    pub fn draws(&self, out: &mut Vec<DrawItem>) {
        for f in self.flights.iter().filter(|f| self.clock >= f.start) {
            f.visual.draws(f.model, out);
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        for batch in self.halos.iter().chain(self.beams.iter()) {
            batch.draw(pass, frame, &self.shared);
        }
    }

    /// The player is on another level: what flew and what was known of the last one goes.
    pub fn clear(&mut self) {
        self.known.clear();
        self.deaths.clear();
        self.flights.clear();
        self.worth.clear();
        self.marks.clear();
    }
}
