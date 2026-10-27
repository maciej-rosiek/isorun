//! Dust that puffs up when an actor lifts off and lands, and footprints that fade, so a walk shows
//! (the port of `steps.ts`).

use crate::combat::Rng;
use crate::gpu::{Blend, Gpu};
use crate::lights::StepSettings;
use crate::particles::{Instance, ParticleBatch, ParticleShared};
use glam::{Mat4, Quat, Vec3};

const MAX_PUFFS: usize = 192;
const MAX_PRINTS: usize = 160;
const LIFT_PUFFS: f32 = 2.0;
const LAND_PUFFS: f32 = 5.0;
const PUFF_HEIGHT: f32 = 0.06;
const PUFF_RISE: f32 = 0.25;
const PUFF_SPREAD: f32 = 0.7;
const PUFF_DRAG: f32 = 3.0;
const PUFF_GROWTH: f32 = 2.2;
const PRINT_HEIGHT: f32 = 0.011;
const PRINT_LENGTH: f32 = 0.22;
const PRINT_WIDTH: f32 = 0.11;
const PRINT_APART: f32 = 0.09;
const PRINT_STAGGER: f32 = 0.06;
const PRINT_FADE: f32 = 0.4;
const PRINT_DARKNESS: f32 = 0.15;

struct Puff {
    position: Vec3,
    velocity: Vec3,
    age: f32,
    life: f32,
    size: f32,
}

struct Print {
    x: f32,
    z: f32,
    angle: f32,
    age: f32,
}

struct Landing {
    delay: f32,
    from: (i32, i32),
    to: (i32, i32),
    floor: bool,
}

pub struct StepLayer {
    settings: StepSettings,
    shared: ParticleShared,
    puffs_batch: ParticleBatch,
    prints_batch: ParticleBatch,
    puffs: Vec<Puff>,
    prints: Vec<Print>,
    landings: Vec<Landing>,
    next_print: usize,
    rng: Rng,
    colour_set: Option<u32>,
}

impl StepLayer {
    /// Dust in the air, a landing to come, or footprints still fading.
    pub fn animating(&self) -> bool {
        !self.puffs.is_empty() || !self.landings.is_empty() || (self.settings.prints > 0.0 && self.prints.iter().any(|p| p.age < self.settings.prints))
    }

    pub fn new(gpu: &Gpu) -> Self {
        let shared = ParticleShared::new(gpu);
        let puffs_batch = ParticleBatch::new(gpu, &shared, "fs_puff", Blend::Alpha, false);
        let prints_batch = ParticleBatch::new(gpu, &shared, "fs_print", Blend::Alpha, false);
        StepLayer {
            settings: StepSettings { colour: 0x8a7a66, dust: 1.0, prints: 6.0 },
            shared,
            puffs_batch,
            prints_batch,
            puffs: Vec::new(),
            prints: Vec::new(),
            landings: Vec::new(),
            next_print: 0,
            rng: Rng::new(0x57e95),
            colour_set: None,
        }
    }

    pub fn prepare(&mut self, gpu: &Gpu, samples: u32) {
        self.puffs_batch.prepare(gpu, &self.shared, samples);
        self.prints_batch.prepare(gpu, &self.shared, samples);
    }

    pub fn configure(&mut self, settings: StepSettings) {
        self.settings = settings;
    }

    /// An actor hops from one cell to the next and lands after `seconds`.
    pub fn step(&mut self, from: (i32, i32), to: (i32, i32), seconds: f64, floor: &dyn Fn(i32, i32) -> bool) {
        if floor(from.0, from.1) {
            self.puff(from, to, LIFT_PUFFS, 0.6);
        }
        self.landings.push(Landing { delay: seconds as f32, from, to, floor: floor(to.0, to.1) });
    }

    pub fn update(&mut self, gpu: &Gpu, dt: f32, right: Vec3, up: Vec3) {
        if self.colour_set != Some(self.settings.colour) {
            self.colour_set = Some(self.settings.colour);
            self.puffs_batch.colour(gpu, self.settings.colour, 1.0);
            self.prints_batch.colour(gpu, self.settings.colour, PRINT_DARKNESS);
        }
        let mut landed = Vec::new();
        self.landings.retain_mut(|l| {
            l.delay -= dt;
            if l.delay > 0.0 {
                return true;
            }
            if l.floor {
                landed.push((l.from, l.to));
            }
            false
        });
        for (from, to) in landed {
            self.puff(to, from, LAND_PUFFS, 1.0);
            self.footprints(from, to);
        }
        for p in &mut self.puffs {
            p.age += dt;
            p.velocity *= (-PUFF_DRAG * dt).exp();
            p.position += p.velocity * dt;
        }
        self.puffs.retain(|p| p.age < p.life);
        let forward = right.cross(up);
        let facing = Quat::from_mat3(&glam::Mat3::from_cols(right, up, -forward));
        let puffs: Vec<Instance> = self
            .puffs
            .iter()
            .map(|p| {
                let t = p.age / p.life;
                let size = p.size * (1.0 + t * PUFF_GROWTH);
                Instance::new(Mat4::from_scale_rotation_translation(Vec3::new(size, size, 1.0), facing, p.position), (t * 6.0).min(1.0) * (1.0 - t) * (1.0 - t))
            })
            .collect();
        self.puffs_batch.set(gpu, &puffs);
        let stay = self.settings.prints;
        let prints: Vec<Instance> = self
            .prints
            .iter_mut()
            .map(|p| {
                p.age += dt;
                let left = if stay > 0.0 { 1.0 - p.age / stay } else { 0.0 };
                let m = Mat4::from_scale_rotation_translation(
                    Vec3::new(PRINT_WIDTH, 1.0, PRINT_LENGTH),
                    Quat::from_rotation_y(p.angle),
                    Vec3::new(p.x, PRINT_HEIGHT, p.z),
                ) * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                Instance::new(m, (left / PRINT_FADE).clamp(0.0, 1.0))
            })
            .collect();
        self.prints_batch.set(gpu, &prints);
    }

    pub fn clear(&mut self) {
        self.puffs.clear();
        self.prints.clear();
        self.landings.clear();
        self.next_print = 0;
    }

    fn puff(&mut self, at: (i32, i32), away: (i32, i32), count: f32, strength: f32) {
        let (dx, dy) = ((at.0 - away.0) as f32, (at.1 - away.1) as f32);
        let len = (dx * dx + dy * dy).sqrt();
        let len = if len == 0.0 { 1.0 } else { len };
        for _ in 0..(count * self.settings.dust).round() as usize {
            if self.puffs.len() >= MAX_PUFFS {
                return;
            }
            let spread = self.rng.range(0.0, std::f32::consts::TAU);
            let velocity =
                Vec3::new(spread.cos() * PUFF_SPREAD + dx / len * 0.3, self.rng.range(0.3, 1.0) * PUFF_RISE, spread.sin() * PUFF_SPREAD + dy / len * 0.3)
                    * strength;
            let position = Vec3::new(at.0 as f32 + 0.5 + self.rng.range(-0.12, 0.12), PUFF_HEIGHT, at.1 as f32 + 0.5 + self.rng.range(-0.12, 0.12));
            let (life, size) = (self.rng.range(0.5, 0.9), self.rng.range(0.12, 0.2) * strength);
            self.puffs.push(Puff { position, velocity, age: 0.0, life, size });
        }
    }

    fn footprints(&mut self, from: (i32, i32), to: (i32, i32)) {
        if self.settings.prints <= 0.0 {
            return;
        }
        let (dx, dy) = ((to.0 - from.0) as f32, (to.1 - from.1) as f32);
        let len = (dx * dx + dy * dy).sqrt();
        let len = if len == 0.0 { 1.0 } else { len };
        let angle = dx.atan2(dy);
        for side in [-1.0f32, 1.0] {
            let along = side * PRINT_STAGGER;
            let x = to.0 as f32 + 0.5 + dx / len * along + dy / len * side * PRINT_APART;
            let z = to.1 as f32 + 0.5 + dy / len * along - dx / len * side * PRINT_APART;
            let print = Print { x, z, angle, age: 0.0 };
            if self.prints.len() < MAX_PRINTS {
                self.prints.push(print);
            } else {
                self.prints[self.next_print] = print;
            }
            self.next_print = (self.next_print + 1) % MAX_PRINTS;
        }
    }

    pub fn draw_prints(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        self.prints_batch.draw(pass, frame, &self.shared);
    }

    pub fn draw_puffs(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        self.puffs_batch.draw(pass, frame, &self.shared);
    }
}
