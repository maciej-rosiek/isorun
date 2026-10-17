//! The things that move and act (the port of `actors.ts`): the player and the monsters with an id,
//! kept from one scene to the next so they hop from cell to cell and play attacks, hits, misses
//! and deaths.

use crate::gpu::{Gpu, Material};
use crate::labels::Motion;
use crate::mesh::{GpuMesh, QuadBatch, ATLAS_INSET, SPRITE_LEAN};
use crate::pages::Pages;
use crate::plan::{ground_basis, is_shown, tile_frame};
use crate::renderer::{Cast, DrawItem, PageRef};
use crate::uniforms::DrawUniform;
use crate::TileSource;
use glam::{EulerRot, Mat4, Quat, Vec3};
use isorun_scene::{Billboard, BillboardKind, Scene, SpriteEffect, TileId, PLAYER_REF};
use std::collections::HashMap;
use std::sync::Arc;

/// The player's actor id.
pub const PLAYER_ACTOR: i64 = PLAYER_REF;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActorAction {
    Attack,
    Hit,
    Miss,
    Death,
}

pub const STEP_SECONDS: f64 = 0.2;
const HOP_HEIGHT: f32 = 0.14;
const LUNGE_SECONDS: f64 = 0.36;
const LUNGE_REACH: f32 = 0.4;
const WINDUP_REACH: f32 = 0.12;
const WINDUP_END: f64 = 0.3;
const STRIKE_END: f64 = 0.45;
/// Seconds from the start of an attack to the moment it reaches the target.
pub const IMPACT_DELAY: f64 = LUNGE_SECONDS * STRIKE_END;
/// Seconds from the start of an attack to the end of its pull back.
pub const STRIKE_DELAY: f64 = LUNGE_SECONDS * WINDUP_END;
const HIT_SECONDS: f64 = 0.3;
const KNOCKBACK: f32 = 0.18;
const DODGE_SECONDS: f64 = 0.3;
const DODGE_DISTANCE: f32 = 0.2;
const DODGE_TILT: f32 = 0.2;
const SQUASH: f32 = 0.25;
const SHAKE_PER_SECOND: f32 = 40.0;
const DEATH_SECONDS: f64 = 0.9;
const DEATH_FLASH_SECONDS: f64 = 0.12;
const DEATH_FALL_SECONDS: f64 = 0.35;
const DEATH_KNOCKBACK: f32 = 0.3;
const DEATH_FALL_ANGLE: f32 = std::f32::consts::PI / 2.2;
const MAX_WALK: i32 = 8;
const CATCH_UP: f64 = 0.5;
const HOP_LATE: f64 = 1.0 / 60.0;
const PACE_EASE: f64 = 0.3;
const FASTEST_PACE: f64 = 0.6;
const SLOWEST_PACE: f64 = 1.5;
const HOP_EASE: f64 = 0.35;
const HURT_TINT: [f32; 3] = [4.0, 0.6, 0.5];
const FLASH_TINT: [f32; 3] = [4.0, 4.0, 4.0];
const LAYER_LIFT: f32 = 0.0015;
const ICON_LIFT: f32 = 0.03;

/// The identity of a billboard that moves and acts: the player, or a monster with crawl's id.
pub fn actor_id(b: &Billboard) -> Option<i64> {
    match b.kind {
        BillboardKind::Player => Some(PLAYER_ACTOR),
        BillboardKind::Monster => b.ref_id,
        _ => None,
    }
}

/// How upright sprites stand: `stretch` up, and `lean` back from the camera in radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UprightPose {
    pub stretch: f32,
    pub lean: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorStep {
    pub id: i64,
    pub from: (i32, i32),
    pub to: (i32, i32),
    /// How long the hop takes, on the actors' clock.
    pub seconds: f64,
}

pub struct ActorEnv<'a> {
    pub gpu: &'a Gpu,
    pub pages: &'a mut Pages,
    pub tiles: &'a dyn TileSource,
}

fn sprite_key(b: &Billboard) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.tile.hash(&mut h);
    for l in &b.layers {
        (l.tile, l.ox.to_bits(), l.oy.to_bits(), l.ymax.map(f32::to_bits)).hash(&mut h);
    }
    for i in &b.status_icons {
        (i.tile, i.ox.to_bits(), i.oy.to_bits(), i.at_top).hash(&mut h);
    }
    h.finish()
}

struct SpriteVisual {
    key: u64,
    pages_id: usize,
    meshes: Vec<(u32, Arc<GpuMesh>)>,
    glows: bool,
    cast_shadow: bool,
    motion: f32,
    phase: f32,
    effects: [f32; 8],
    tint: [f32; 3],
    opacity: f32,
}

impl SpriteVisual {
    fn new(b: &Billboard, env: &mut ActorEnv, key: u64, cast_shadow: bool, phase: f32) -> Self {
        let mut effects = [0.0; 8];
        for e in &b.effects {
            let i = match e {
                SpriteEffect::Petrified => 0,
                SpriteEffect::Burning => 1,
                SpriteEffect::Poisoned => 2,
                SpriteEffect::Corroded => 3,
                SpriteEffect::Berserk => 4,
                SpriteEffect::Glowing => 5,
                SpriteEffect::Summoned => 6,
                SpriteEffect::Wet => 7,
            };
            effects[i] = 1.0;
        }
        // the sprite's layers, then its status icons (an icon pinned to the top stands on the frame's top edge)
        let mut layers: Vec<(TileId, f32, f32, Option<f32>, bool)> = if b.layers.is_empty() { vec![(b.tile, 0.0, 0.0, None, false)] } else { b.layers.iter().map(|l| (l.tile, l.ox, l.oy, l.ymax, false)).collect() };
        for i in &b.status_icons {
            if i.square {
                continue;
            }
            let Some(rect) = env.tiles.tile(i.tile) else { continue };
            layers.push((i.tile, i.ox, if i.at_top { -(rect.oy as f32) } else { i.oy }, None, true));
        }
        let texels = env.pages.page_texels();
        let mut batches: HashMap<u32, QuadBatch> = HashMap::new();
        let mut glows = false;
        let mut motion = 0.0;
        for (index, (tile, ox, oy, ymax, icon)) in layers.iter().enumerate() {
            let Some(rect) = env.tiles.tile(*tile) else { continue };
            let Some(placed) = env.pages.for_tile(&env.gpu.device, &env.gpu.page_layout, &env.gpu.page_sampler, env.tiles, &rect) else { continue };
            if placed.label.is_some_and(|l| l.glow > 0) {
                glows = true;
            }
            if index == 0 {
                if let Some(l) = placed.label {
                    motion = match l.motion {
                        Motion::None => 0.0,
                        Motion::Breathe => 1.0,
                        Motion::Hover => 2.0,
                        Motion::Flicker => 3.0,
                    };
                }
            }
            let f = tile_frame(&rect, *ox, *oy, *ymax);
            if f.height <= 0.0 {
                continue;
            }
            let left = f.left - 0.5;
            let right = left + f.width;
            let top = 1.0 - f.top;
            let bottom = 1.0 - f.top - f.height;
            let z = if *icon { ICON_LIFT } else { index as f32 * LAYER_LIFT };
            let batch = batches.entry(placed.page).or_insert_with(|| QuadBatch::new(texels, texels, SPRITE_LEAN, ATLAS_INSET));
            batch.add_grey([[left, top, z], [right, top, z], [right, bottom, z], [left, bottom, z]], placed.sx, placed.sy, rect.w as f32, f.texel_height, 1.0);
        }
        let mut meshes: Vec<(u32, Arc<GpuMesh>)> = batches.into_iter().map(|(p, b)| (p, Arc::new(GpuMesh::from_batch(&env.gpu.device, &b)))).collect();
        meshes.sort_by_key(|m| m.0);
        SpriteVisual { key, pages_id: env.pages as *const Pages as usize, meshes, glows, cast_shadow, motion, phase, effects, tint: [1.0; 3], opacity: 1.0 }
    }

    fn set_flash(&mut self, amount: f32, tint: [f32; 3]) {
        self.tint = [1.0 + amount * tint[0], 1.0 + amount * (tint[1] - 1.0), 1.0 + amount * (tint[2] - 1.0)];
    }
}

struct Actor {
    id: i64,
    visual: SpriteVisual,
    x: i32,
    y: i32,
    hop_from: (i32, i32),
    hop_to: (i32, i32),
    hop_start: f64,
    hop_seconds: f64,
    pace: f64,
    last_arrival: f64,
    path: Vec<(i32, i32)>,
    started: Vec<((i32, i32), (i32, i32), f64)>,
    lunge: Option<(f64, f32, f32)>,
    hit: (f64, f32, f32),
    dodge: (f64, f32, f32),
    fall: (f32, f32),
    death_start: Option<f64>,
    moving: bool,
    phase: f32,
    last_now: f64,
    model: Mat4,
}

fn random_phase(seed: i64) -> f32 {
    let mut s = (seed as u64).wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(0x632BE59BD9B4E019);
    s ^= s >> 31;
    (s % 10_000) as f32 / 10_000.0 * std::f32::consts::TAU
}

impl Actor {
    fn new(id: i64, visual: SpriteVisual, b: &Billboard) -> Self {
        Actor {
            id,
            visual,
            x: b.x,
            y: b.y,
            hop_from: (b.x, b.y),
            hop_to: (b.x, b.y),
            hop_start: f64::NEG_INFINITY,
            hop_seconds: STEP_SECONDS,
            pace: STEP_SECONDS,
            last_arrival: f64::NEG_INFINITY,
            path: Vec::new(),
            started: Vec::new(),
            lunge: None,
            hit: (f64::NEG_INFINITY, 0.0, 0.0),
            dodge: (f64::NEG_INFINITY, 0.0, 0.0),
            fall: (0.0, 0.0),
            death_start: None,
            moving: false,
            phase: random_phase(id.wrapping_mul(31).wrapping_add(7)),
            last_now: 0.0,
            model: Mat4::IDENTITY,
        }
    }

    /// The actor is on cell `x`, `y` now: a walk of a few cells hops through the cells between; a longer move is a jump.
    fn move_to(&mut self, x: i32, y: i32, now: f64) {
        if x == self.x && y == self.y {
            return;
        }
        let cells = (x - self.x).abs().max((y - self.y).abs());
        if cells > MAX_WALK {
            self.path.clear();
            self.hop_from = (x, y);
            self.hop_to = (x, y);
            self.hop_start = f64::NEG_INFINITY;
        } else {
            self.keep_pace((now - self.last_arrival) / cells as f64);
            self.last_arrival = now;
            let mut at = (self.x, self.y);
            while at != (x, y) {
                at = (at.0 + (x - at.0).signum(), at.1 + (y - at.1).signum());
                self.path.push(at);
            }
        }
        self.x = x;
        self.y = y;
        self.advance(now);
    }

    fn keep_pace(&mut self, seconds_per_cell: f64) {
        if seconds_per_cell > STEP_SECONDS * SLOWEST_PACE {
            self.pace = STEP_SECONDS;
        } else {
            self.pace += ((STEP_SECONDS * FASTEST_PACE).max(seconds_per_cell) - self.pace) * PACE_EASE;
        }
    }

    /// Start the next hop of the path once the one under way has landed.
    fn advance(&mut self, now: f64) {
        while !self.path.is_empty() && now >= self.hop_start + self.hop_seconds {
            let end = self.hop_start + self.hop_seconds;
            let next = self.path.remove(0);
            let continuing = now - end < self.hop_seconds;
            self.hop_start = end.max(now - HOP_LATE);
            let wanted = self.pace / (1.0 + self.path.len() as f64 * CATCH_UP);
            self.hop_seconds = if continuing { self.hop_seconds + (wanted - self.hop_seconds) * HOP_EASE } else { wanted };
            self.hop_from = self.hop_to;
            self.hop_to = next;
            self.started.push((self.hop_from, next, self.hop_seconds));
        }
    }

    fn act(&mut self, action: ActorAction, now: f64, target: Option<(i32, i32)>) {
        if self.death_start.is_some() {
            return;
        }
        let (dx, dy) = match target {
            Some((tx, ty)) => {
                let (dx, dy) = ((tx - self.x) as f32, (ty - self.y) as f32);
                let len = (dx * dx + dy * dy).sqrt();
                let len = if len == 0.0 { 1.0 } else { len };
                (dx / len, dy / len)
            }
            None => (0.0, 0.0),
        };
        match action {
            ActorAction::Attack if target.is_some() => self.lunge = Some((now, dx, dy)),
            ActorAction::Hit => self.hit = (now, -dx, -dy),
            ActorAction::Miss => self.dodge = (now, -dy, dx),
            ActorAction::Death => {
                self.death_start = Some(now);
                self.fall = (-dx, -dy);
            }
            _ => {}
        }
    }

    fn impact_time(&self) -> Option<f64> {
        self.lunge.map(|l| l.0 + IMPACT_DELAY)
    }

    fn finished(&self) -> bool {
        self.death_start.is_some_and(|d| self.last_now - d > DEATH_SECONDS)
    }

    fn shown_cell(&self, now: f64) -> (f32, f32) {
        let t = ((now - self.hop_start) / self.hop_seconds).clamp(0.0, 1.0) as f32;
        let eased = t * t * (3.0 - 2.0 * t);
        (self.hop_from.0 as f32 + (self.hop_to.0 - self.hop_from.0) as f32 * eased, self.hop_from.1 as f32 + (self.hop_to.1 - self.hop_from.1) as f32 * eased)
    }

    fn update(&mut self, now: f64, yaw: f32, pose: UprightPose, ground_at: &dyn Fn(i32, i32) -> f32) {
        self.last_now = now;
        self.advance(now);
        let move_t = ((now - self.hop_start) / self.hop_seconds).min(1.0);
        let cell = self.shown_cell(now);
        let mut px = cell.0 + 0.5;
        let mut pz = cell.1 + 0.5;
        let lift = if move_t < 1.0 { (std::f64::consts::PI * move_t).sin() as f32 * HOP_HEIGHT * (self.hop_seconds / STEP_SECONDS).min(1.0) as f32 } else { 0.0 };
        self.moving = move_t < 1.0 || !self.path.is_empty() || self.lunge.is_some() || self.death_start.is_some();
        if let Some((start, dx, dy)) = self.lunge {
            let t = (now - start) / LUNGE_SECONDS;
            if t >= 1.0 {
                self.lunge = None;
            } else {
                let reach = lunge_reach(t);
                px += dx * reach;
                pz += dy * reach;
            }
        }
        let hit_t = ((now - self.hit.0) / HIT_SECONDS) as f32;
        let hitting = (0.0..1.0).contains(&hit_t);
        let recoil = if hitting { (1.0 - hit_t) * (1.0 - hit_t) } else { 0.0 };
        self.moving |= hitting;
        if hitting {
            let push = (std::f32::consts::PI * (hit_t * 2.0).min(1.0)).sin() * KNOCKBACK * (1.0 - hit_t);
            px += self.hit.1 * push + (hit_t * SHAKE_PER_SECOND).sin() * 0.04 * (1.0 - hit_t);
            pz += self.hit.2 * push;
        }
        let dodge_t = ((now - self.dodge.0) / DODGE_SECONDS) as f32;
        let dodging = (0.0..1.0).contains(&dodge_t);
        self.moving |= dodging;
        let sway = if dodging { (std::f32::consts::PI * dodge_t).sin() } else { 0.0 };
        px += self.dodge.1 * sway * DODGE_DISTANCE;
        pz += self.dodge.2 * sway * DODGE_DISTANCE;
        let mut flash = if hitting { 1.0 - hit_t } else { 0.0 };
        let (right, _) = ground_basis(yaw);
        let mut tilt = sway * DODGE_TILT * if self.dodge.1 * right[0] + self.dodge.2 * right[1] < 0.0 { 1.0 } else { -1.0 };
        let mut opacity = 1.0;
        let mut sink = 1.0;
        if let Some(death) = self.death_start {
            let age = (now - death).max(0.0);
            flash = flash.max(if age < DEATH_FLASH_SECONDS { 1.0 } else { 0.0 });
            let fall_t = (age / DEATH_FALL_SECONDS).min(1.0) as f32;
            let eased = 1.0 - (1.0 - fall_t) * (1.0 - fall_t);
            px += self.fall.0 * DEATH_KNOCKBACK * eased;
            pz += self.fall.1 * DEATH_KNOCKBACK * eased;
            let side = if self.fall.0 * right[0] + self.fall.1 * right[1] < 0.0 { 1.0 } else { -1.0 };
            tilt = side * DEATH_FALL_ANGLE * fall_t * fall_t;
            let fade_t = ((age - DEATH_FALL_SECONDS) / (DEATH_SECONDS - DEATH_FALL_SECONDS)).max(0.0) as f32;
            opacity = 1.0 - fade_t;
            sink = 1.0 - fade_t * 0.5;
        }
        self.visual.set_flash(flash, if self.id == PLAYER_ACTOR { HURT_TINT } else { FLASH_TINT });
        self.visual.opacity = opacity;
        let g_from = ground_at(self.hop_from.0, self.hop_from.1);
        let g_to = ground_at(self.hop_to.0, self.hop_to.1);
        let ground = g_from + (g_to - g_from) * move_t.clamp(0.0, 1.0) as f32;
        let position = Vec3::new(px, lift + ground, pz);
        let breath = ((now as f32) * 2.6 + self.phase).sin() * 0.025;
        let scale_y = pose.stretch * (1.0 + breath) * (1.0 - recoil * SQUASH) * sink;
        let scale = Vec3::new((1.0 - breath * 0.5) * (1.0 + recoil * SQUASH), scale_y, 1.0);
        let rotation = Quat::from_euler(EulerRot::YXZ, -yaw, -pose.lean, tilt);
        self.model = Mat4::from_translation(position) * Mat4::from_quat(rotation) * Mat4::from_scale(scale);
    }
}

/// How far an attack reaches at `t` (0 to 1): a short pull back, a fast strike, a slower return.
fn lunge_reach(t: f64) -> f32 {
    if t < WINDUP_END {
        return -(std::f64::consts::PI * t / WINDUP_END / 2.0).sin() as f32 * WINDUP_REACH;
    }
    if t < STRIKE_END {
        let u = ((t - WINDUP_END) / (STRIKE_END - WINDUP_END)) as f32;
        return -WINDUP_REACH + u * u * (LUNGE_REACH + WINDUP_REACH);
    }
    let u = ((t - STRIKE_END) / (1.0 - STRIKE_END)) as f32;
    LUNGE_REACH * (1.0 - u) * (1.0 - u)
}

#[derive(Default)]
pub struct ActorLayer {
    actors: HashMap<i64, Actor>,
    shadow_change: bool,
}

impl ActorLayer {
    pub fn sync(&mut self, scene: &Scene, now: f64, env: &mut ActorEnv) {
        let mut seen = Vec::new();
        let pages_id = env.pages as *const Pages as usize;
        for b in &scene.billboards {
            let Some(id) = actor_id(b) else { continue };
            if !is_shown(b, scene) {
                continue;
            }
            seen.push(id);
            let key = sprite_key(b);
            match self.actors.get_mut(&id) {
                None => {
                    let visual = SpriteVisual::new(b, env, key, id != PLAYER_ACTOR, random_phase(id));
                    self.actors.insert(id, Actor::new(id, visual, b));
                    self.shadow_change = true;
                }
                Some(actor) if actor.visual.key != key || actor.visual.pages_id != pages_id => {
                    actor.visual = SpriteVisual::new(b, env, key, id != PLAYER_ACTOR, actor.visual.phase);
                    self.shadow_change = true;
                }
                _ => {}
            }
            if let Some(actor) = self.actors.get_mut(&id) {
                actor.move_to(b.x, b.y, now);
            }
        }
        let gone: Vec<i64> = self.actors.iter().filter(|(id, a)| !seen.contains(id) && a.death_start.is_none()).map(|(id, _)| *id).collect();
        for id in gone {
            self.actors.remove(&id);
            self.shadow_change = true;
        }
    }

    pub fn act(&mut self, id: i64, action: ActorAction, now: f64, target: Option<(i32, i32)>) {
        if let Some(a) = self.actors.get_mut(&id) {
            a.act(action, now, target);
        }
    }

    /// When a blow from the actor on `cell` lands: when its attack under way reaches the target, or a whole attack from `now`.
    pub fn impact_from(&self, cell: Option<(i32, i32)>, now: f64) -> f64 {
        let attacker = cell.and_then(|c| self.actors.values().find(|a| (a.x, a.y) == c && a.death_start.is_none()));
        match attacker.and_then(|a| a.impact_time()) {
            Some(t) => now.max(t),
            None => now + IMPACT_DELAY,
        }
    }

    pub fn shown_cell_of(&self, id: i64) -> Option<(f32, f32)> {
        self.actors.get(&id).map(|a| a.shown_cell(a.last_now))
    }

    pub fn cell_of(&self, id: i64) -> Option<(i32, i32)> {
        self.actors.get(&id).map(|a| (a.x, a.y))
    }

    /// Moves every actor on to `now`; returns the hops that started.
    pub fn update(&mut self, now: f64, yaw: f32, pose: UprightPose, ground_at: &dyn Fn(i32, i32) -> f32) -> Vec<ActorStep> {
        let mut steps = Vec::new();
        let mut finished = Vec::new();
        for (id, actor) in self.actors.iter_mut() {
            let was_moving = actor.moving;
            actor.update(now, yaw, pose, ground_at);
            for (from, to, seconds) in actor.started.drain(..) {
                steps.push(ActorStep { id: *id, from, to, seconds });
            }
            if actor.visual.cast_shadow && (was_moving || actor.moving) {
                self.shadow_change = true;
            }
            if actor.finished() {
                finished.push(*id);
            }
        }
        for id in finished {
            self.actors.remove(&id);
            self.shadow_change = true;
        }
        steps.sort_by_key(|s| s.id);
        steps
    }

    pub fn glowing(&self) -> bool {
        self.actors.values().any(|a| a.visual.glows)
    }

    pub fn take_shadow_change(&mut self) -> bool {
        std::mem::take(&mut self.shadow_change)
    }

    pub fn clear(&mut self) {
        self.actors.clear();
        self.shadow_change = true;
    }

    pub fn draws(&self, out: &mut Vec<DrawItem>) {
        let mut ids: Vec<&i64> = self.actors.keys().collect();
        ids.sort();
        for id in ids {
            let a = &self.actors[id];
            let v = &a.visual;
            let uniform = DrawUniform {
                model: a.model.to_cols_array_2d(),
                tint: [v.tint[0], v.tint[1], v.tint[2], v.opacity],
                look: [v.motion, v.phase, 0.0, 1.0],
                harm: [v.effects[0], v.effects[1], v.effects[2], v.effects[3]],
                aura: [v.effects[4], v.effects[5], v.effects[6], v.effects[7]],
            };
            for (page, mesh) in &v.meshes {
                out.push(DrawItem { material: Material::Actor, page: PageRef::Page(*page), mesh: mesh.clone(), uniform, cast: if v.cast_shadow { Cast::Actor } else { Cast::None } });
            }
        }
    }
}
