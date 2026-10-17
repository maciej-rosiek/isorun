//! The blood, the sparks, the flash of light, the shake and the stop that make a hit felt (the
//! port of `combat.ts`).

use crate::gpu::{Blend, Gpu};
use crate::lights::CombatSettings;
use crate::particles::{Instance, ParticleBatch, ParticleShared};
use glam::{Mat4, Quat, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImpactKind {
    Hit,
    Death,
    Hurt,
}

struct Spec {
    drops: f32,
    drop_speed: f32,
    sparks: f32,
    trauma: f32,
    punch: f32,
    light: f32,
    pool: f32,
    stop: f32,
}

fn spec(kind: ImpactKind) -> Spec {
    match kind {
        ImpactKind::Hit => Spec { drops: 10.0, drop_speed: 2.2, sparks: 8.0, trauma: 0.4, punch: 0.3, light: 1.0, pool: 0.0, stop: 1.0 },
        ImpactKind::Hurt => Spec { drops: 8.0, drop_speed: 2.0, sparks: 6.0, trauma: 0.6, punch: 0.5, light: 1.0, pool: 0.0, stop: 1.0 },
        ImpactKind::Death => Spec { drops: 30.0, drop_speed: 3.0, sparks: 16.0, trauma: 0.8, punch: 1.0, light: 1.8, pool: 0.9, stop: 1.8 },
    }
}

const MAX_DROPS: usize = 256;
const MAX_SPARKS: usize = 128;
const MAX_SPLATS: usize = 160;
const GRAVITY: f32 = 9.8;
const SPARK_GRAVITY: f32 = 3.0;
const IMPACT_HEIGHT: f32 = 0.45;
const SPLAT_HEIGHT: f32 = 0.012;
const DROP_SPLAT_SCALE: f32 = 3.5;
const POOL_GROW_SECONDS: f32 = 0.8;
const DROP_GROW_SECONDS: f32 = 0.08;
const STREAK: f32 = 0.035;
const TRAUMA_DECAY: f32 = 1.8;
const PUNCH_DECAY: f32 = 6.0;
const LIGHT_DECAY: f32 = 14.0;
const SHAKE_CELLS: f32 = 0.18;
const SHAKE_ROLL: f32 = 1.5 * std::f32::consts::PI / 180.0;
const SHAKE_FREQUENCY: f32 = 31.0;
const PUNCH_ZOOM: f32 = 0.05;
const LIGHT_INTENSITY: f32 = 10.0;

/// A small generator for the effects' scatter: it need not be Math.random, only look random.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next() * (max - min)
    }
}

#[derive(Clone, Copy)]
struct Particle {
    position: Vec3,
    velocity: Vec3,
    age: f32,
    life: f32,
    size: f32,
}

#[derive(Clone, Copy)]
struct Splat {
    x: f32,
    z: f32,
    size: f32,
    angle: f32,
    age: f32,
    grow: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shake {
    pub x: f32,
    pub y: f32,
    pub roll: f32,
    pub zoom: f32,
}

pub struct CombatLayer {
    settings: CombatSettings,
    shared: ParticleShared,
    drops_batch: ParticleBatch,
    sparks_batch: ParticleBatch,
    splats_batch: ParticleBatch,
    drops: Vec<Particle>,
    sparks: Vec<Particle>,
    splats: Vec<Splat>,
    next_splat: usize,
    trauma: f32,
    punch: f32,
    light_amount: f32,
    light_at: Vec3,
    time: f32,
    rng: Rng,
    colour_set: Option<u32>,
}

impl CombatLayer {
    pub fn new(gpu: &Gpu) -> Self {
        let shared = ParticleShared::new(gpu);
        let drops_batch = ParticleBatch::new(gpu, &shared, "fs_flat", Blend::Opaque, true);
        let sparks_batch = ParticleBatch::new(gpu, &shared, "fs_flat", Blend::Additive, false);
        let splats_batch = ParticleBatch::new(gpu, &shared, "fs_splat", Blend::Alpha, false);
        sparks_batch.colour(gpu, 0xffe6b0, 1.0);
        CombatLayer {
            settings: CombatSettings { blood: 0x8a0f0f, gore: 1.0, shake: 0.4, hit_stop: 0.07, flash: 1.0 },
            shared,
            drops_batch,
            sparks_batch,
            splats_batch,
            drops: Vec::new(),
            sparks: Vec::new(),
            splats: Vec::new(),
            next_splat: 0,
            trauma: 0.0,
            punch: 0.0,
            light_amount: 0.0,
            light_at: Vec3::ZERO,
            time: 0.0,
            rng: Rng::new(0x5eed_1234),
            colour_set: None,
        }
    }

    pub fn prepare(&mut self, gpu: &Gpu, samples: u32) {
        self.drops_batch.prepare(gpu, &self.shared, samples);
        self.sparks_batch.prepare(gpu, &self.shared, samples);
        self.splats_batch.prepare(gpu, &self.shared, samples);
    }

    pub fn configure(&mut self, settings: CombatSettings) {
        self.settings = settings;
    }

    /// Spill blood and sparks at `at`, away from the attacker at `from`, flash a light and shake the
    /// camera. Returns how many seconds the actors stop.
    pub fn impact(&mut self, kind: ImpactKind, at: (i32, i32), from: Option<(i32, i32)>, floor: &dyn Fn(i32, i32) -> bool) -> f64 {
        let s = spec(kind);
        let CombatSettings { gore, flash, shake, hit_stop, .. } = self.settings;
        let origin = Vec3::new(at.0 as f32 + 0.5, IMPACT_HEIGHT, at.1 as f32 + 0.5);
        let away = match from {
            Some(f) => Vec3::new((at.0 - f.0) as f32, 0.0, (at.1 - f.1) as f32),
            None => Vec3::new(self.rng.range(-1.0, 1.0), 0.0, self.rng.range(-1.0, 1.0)),
        }
        .normalize_or_zero();
        for _ in 0..(s.drops * gore).round() as usize {
            let mut v = away * (self.rng.range(0.4, 1.0) * s.drop_speed);
            v.x += self.rng.range(-1.0, 1.0);
            v.z += self.rng.range(-1.0, 1.0);
            v.y = self.rng.range(0.8, 2.6);
            if self.drops.len() < MAX_DROPS {
                let size = self.rng.range(0.035, 0.075);
                self.drops.push(Particle { position: origin, velocity: v, age: 0.0, life: f32::INFINITY, size });
            }
        }
        for _ in 0..(s.sparks * flash.min(1.0)).round() as usize {
            let mut v = Vec3::new(self.rng.range(-1.0, 1.0), self.rng.range(-0.2, 1.0), self.rng.range(-1.0, 1.0)).normalize_or_zero() * self.rng.range(2.5, 5.5);
            v -= away * 1.5;
            if self.sparks.len() < MAX_SPARKS {
                let (life, size) = (self.rng.range(0.12, 0.28), self.rng.range(0.02, 0.04));
                self.sparks.push(Particle { position: origin, velocity: v, age: 0.0, life, size });
            }
        }
        if s.pool > 0.0 && gore > 0.0 && floor(at.0, at.1) {
            self.splat(origin.x, origin.z, s.pool * gore.min(1.5), POOL_GROW_SECONDS);
        }
        self.trauma = (self.trauma + s.trauma * shake).min(1.0);
        self.punch = (self.punch + s.punch * shake).min(1.0);
        self.light_amount = s.light * flash;
        self.light_at = Vec3::new(origin.x, IMPACT_HEIGHT + 0.3, origin.z);
        (hit_stop * s.stop) as f64
    }

    fn splat(&mut self, x: f32, z: f32, size: f32, grow: f32) {
        let splat = Splat { x, z, size, angle: self.rng.range(0.0, std::f32::consts::TAU), age: 0.0, grow };
        if self.splats.len() < MAX_SPLATS {
            self.splats.push(splat);
        } else {
            self.splats[self.next_splat] = splat;
        }
        self.next_splat = (self.next_splat + 1) % MAX_SPLATS;
    }

    /// Move the blood and the sparks on, every particle facing the camera.
    pub fn update(&mut self, gpu: &Gpu, dt: f32, right: Vec3, up: Vec3, floor: &dyn Fn(i32, i32) -> bool) {
        if self.colour_set != Some(self.settings.blood) {
            self.colour_set = Some(self.settings.blood);
            self.drops_batch.colour(gpu, self.settings.blood, 1.0);
            self.splats_batch.colour(gpu, self.settings.blood, 0.7);
        }
        self.time += dt;
        self.trauma = (self.trauma - TRAUMA_DECAY * dt).max(0.0);
        self.punch *= (-PUNCH_DECAY * dt).exp();
        self.light_amount *= (-LIGHT_DECAY * dt).exp();
        let mut landed = Vec::new();
        for d in &mut self.drops {
            d.velocity.y -= GRAVITY * dt;
            d.position += d.velocity * dt;
            if d.position.y > 0.0 {
                continue;
            }
            d.age = d.life;
            if floor(d.position.x.floor() as i32, d.position.z.floor() as i32) {
                landed.push((d.position.x, d.position.z, d.size * DROP_SPLAT_SCALE));
            }
        }
        for (x, z, size) in landed {
            self.splat(x, z, size, DROP_GROW_SECONDS);
        }
        for s in &mut self.sparks {
            s.age += dt;
            s.velocity.y -= SPARK_GRAVITY * dt;
            s.velocity *= (-3.0 * dt).exp();
            s.position += s.velocity * dt;
        }
        self.drops.retain(|p| p.age < p.life);
        self.sparks.retain(|p| p.age < p.life);
        let forward = right.cross(up);
        let facing = Quat::from_mat3(&glam::Mat3::from_cols(right, up, -forward));
        self.drops_batch.set(gpu, &streaks(&self.drops, facing, right, up, 1.0));
        self.sparks_batch.set(gpu, &streaks(&self.sparks, facing, right, up, 2.5));
        let mut splats = Vec::with_capacity(self.splats.len());
        for s in &mut self.splats {
            if s.age < s.grow {
                s.age += dt;
            }
            let grown = s.size * (s.age / s.grow).min(1.0).sqrt();
            let m = Mat4::from_scale_rotation_translation(Vec3::new(grown, 1.0, grown * 0.8), Quat::from_rotation_y(s.angle), Vec3::new(s.x, SPLAT_HEIGHT, s.z)) * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            splats.push(Instance::new(m, 1.0));
        }
        self.splats_batch.set(gpu, &splats);
    }

    /// How far the camera moves and turns now.
    pub fn shake(&self) -> Shake {
        let amount = self.trauma * self.trauma;
        let t = self.time * SHAKE_FREQUENCY;
        Shake {
            x: amount * SHAKE_CELLS * (t.sin() * 0.6 + (t * 2.3 + 1.7).sin() * 0.4),
            y: amount * SHAKE_CELLS * ((t * 1.3 + 0.5).sin() * 0.6 + (t * 2.9 + 4.1).sin() * 0.4),
            roll: amount * SHAKE_ROLL * (t * 0.9 + 2.2).sin(),
            zoom: 1.0 + self.punch * PUNCH_ZOOM,
        }
    }

    /// The flash of a hit as a light: where, and how bright.
    pub fn light(&self) -> Option<(Vec3, f32)> {
        let intensity = self.light_amount * LIGHT_INTENSITY;
        (intensity > 1e-3).then_some((self.light_at, intensity))
    }

    pub fn clear(&mut self) {
        self.drops.clear();
        self.sparks.clear();
        self.splats.clear();
        self.next_splat = 0;
    }

    pub fn draw_opaque(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        self.drops_batch.draw(pass, frame, &self.shared);
    }

    pub fn draw_sparks(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        self.sparks_batch.draw(pass, frame, &self.shared);
    }

    pub fn draw_splats(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        self.splats_batch.draw(pass, frame, &self.shared);
    }
}

/// Every particle a small square facing the camera, stretched along the way it moves on the screen.
fn streaks(list: &[Particle], facing: Quat, right: Vec3, up: Vec3, stretch: f32) -> Vec<Instance> {
    list.iter()
        .map(|p| {
            let across = p.velocity.dot(right);
            let along = p.velocity.dot(up);
            let fade = if p.life.is_finite() { 1.0 - p.age / p.life } else { 1.0 };
            let rotation = facing * Quat::from_rotation_z(along.atan2(across));
            let size = p.size * fade;
            let m = Mat4::from_scale_rotation_translation(Vec3::new(size + (across * across + along * along).sqrt() * STREAK * stretch, size, 1.0), rotation, p.position);
            Instance::new(m, 1.0)
        })
        .collect()
}
