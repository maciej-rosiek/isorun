//! Numbers that pop over the actors as blows land (damage-numbers.ts, .css): the damage a monster
//! took, the damage the player took sized by how much of their life it was, misses, kills and
//! healing. Each follows its cell on the screen (the renderer's `screen_of`) until it fades; the
//! game screen asks for repaints only while one is up (`alive`).

use crate::paint::{hex, C};
use crate::style::{self, Face};
use egui::{Color32, Painter, Pos2};

/// The blow the renderer is showing (combat.ts `CombatEvent`, the parts that matter here).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Landing {
    Attack,
    /// a hit: on the player or not, crawl's "!"s (`tier`), the amount when the line says it
    Hit {
        on_player: bool,
        tier: u32,
        amount: Option<i64>,
    },
    Miss,
    Death {
        on_player: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Dealt,
    Taken,
    Miss,
    Kill,
    Heal,
}

/// What floats up over an actor: its text, its colour, how big it is and whether it shakes.
#[derive(Clone, Debug, PartialEq)]
pub struct Floater {
    pub text: String,
    pub tone: Tone,
    pub scale: f32,
    pub shake: bool,
}

const LIFE: f64 = 0.9;
const MAX_ALIVE: usize = 12;
const DRIFT_PX: f32 = 22.0;
const STACK_PX: f32 = 18.0;
const STACK: f64 = 0.35;
/// How long a drop of the player's hit points waits for the blow that explains it.
const CLAIM: f64 = 1.5;
/// A blow that takes this share of the player's most hit points is big, and the next one very big.
const HEAVY_SHARE: f32 = 0.2;
const DEADLY_SHARE: f32 = 0.35;
/// Hit points coming back at once, at least this many and this share of the most, read as healing.
const HEAL_MIN: i64 = 5;
const HEAL_SHARE: f32 = 0.15;
const TIER_SCALES: [f32; 4] = [1.0, 1.15, 1.35, 1.7];
/// A blow whose line has no amount shows only from this many "!"s: weaker ones would be noise.
const UNNUMBERED_TIER: usize = 2;
/// How long a blow on the player waits for the drop of the hit points the server sends after its line.
const DROP_WAIT: f64 = 0.25;

/// The damage a monster took: its amount, or crawl's "!"s for a strong blow when the line does
/// not say; none for a weaker one.
pub fn dealt_floater(tier: u32, amount: Option<i64>) -> Option<Floater> {
    let strength = (tier as usize).min(TIER_SCALES.len() - 1);
    if amount == Some(0) {
        return Some(Floater { text: "0".into(), tone: Tone::Miss, scale: 0.9, shake: false });
    }
    if amount.is_none() && strength < UNNUMBERED_TIER {
        return None;
    }
    Some(Floater {
        text: amount.map_or_else(|| "!".repeat(strength), |a| a.to_string()),
        tone: Tone::Dealt,
        scale: TIER_SCALES[strength],
        shake: strength == TIER_SCALES.len() - 1,
    })
}

/// The damage the player took, as big as the share of the most hit points it took.
pub fn taken_floater(amount: i64, hp_max: i64) -> Floater {
    let share = if hp_max > 0 { amount as f32 / hp_max as f32 } else { 0.0 };
    let scale = if share >= DEADLY_SHARE {
        2.0
    } else if share >= HEAVY_SHARE {
        1.55
    } else if share >= HEAVY_SHARE / 2.0 {
        1.2
    } else {
        1.0
    };
    Floater { text: format!("-{amount}"), tone: Tone::Taken, scale, shake: share >= HEAVY_SHARE }
}

struct Alive {
    f: Floater,
    cell: (i32, i32),
    lift: f32,
    born: f64,
    dx: f32,
}

#[derive(Default)]
pub struct DamageNumbers {
    alive: Vec<Alive>,
    hp: Option<i64>,
    hp_max: i64,
    drops: Vec<(i64, f64)>,
    /// blows on the player waiting a moment for the drop that explains them
    waiting: Vec<(Option<i64>, (i32, i32), f64)>,
    seed: u32,
}

impl DamageNumbers {
    /// The player's hit points as the server last said: a drop waits for the blow that explains
    /// it, and a big rise at once shows as healing. `enabled` is the Damage numbers setting.
    pub fn health(&mut self, hp: i64, hp_max: i64, at: (i32, i32), now: f64, enabled: bool) {
        let was = self.hp;
        let same_max = hp_max == self.hp_max;
        self.hp = Some(hp);
        self.hp_max = hp_max;
        self.drops.retain(|d| now - d.1 < CLAIM);
        let Some(was) = was.filter(|_| same_max) else { return };
        if hp < was {
            self.drops.push((was - hp, now));
        } else if hp - was >= HEAL_MIN.max((hp_max as f32 * HEAL_SHARE) as i64) && enabled && hp != was {
            self.spawn(at, Floater { text: format!("+{}", hp - was), tone: Tone::Heal, scale: 1.2, shake: false }, now);
        }
    }

    /// The blow of `e` lands on `at` now (`IsoRender3d.act`'s `onShown`).
    pub fn landing(&mut self, e: Landing, at: (i32, i32), now: f64, enabled: bool) {
        if !enabled {
            return;
        }
        match e {
            Landing::Attack => {}
            Landing::Miss => self.spawn(at, Floater { text: "miss".into(), tone: Tone::Miss, scale: 0.8, shake: false }, now),
            Landing::Death { on_player: false } => self.spawn(at, Floater { text: "KILL".into(), tone: Tone::Kill, scale: 1.1, shake: false }, now),
            Landing::Death { on_player: true } => {}
            Landing::Hit { on_player: false, tier, amount } => {
                if let Some(f) = dealt_floater(tier, amount) {
                    self.spawn(at, f, now);
                }
            }
            Landing::Hit { on_player: true, amount, .. } => {
                if !self.show_taken(amount, at, now) {
                    self.waiting.push((amount, at, now));
                }
            }
        }
    }

    fn show_taken(&mut self, said: Option<i64>, at: (i32, i32), now: f64) -> bool {
        match self.claim_drops(said, now) {
            Some(taken) => {
                // a blow that did no harm reads as one on a monster does: "0", not "-0"
                let f = if taken <= 0 { Floater { text: "0".into(), tone: Tone::Miss, scale: 0.9, shake: false } } else { taken_floater(taken, self.hp_max) };
                self.spawn(at, f, now);
                true
            }
            None => false,
        }
    }

    /// The damage a blow on the player did: what its line says, taken off the drops waiting, or
    /// else all the drops waiting.
    fn claim_drops(&mut self, said: Option<i64>, now: f64) -> Option<i64> {
        let waiting: i64 = self.drops.iter().filter(|d| now - d.1 < CLAIM).map(|d| d.0).sum();
        let left = said.map_or(0, |s| (waiting - s).max(0));
        self.drops = if left > 0 { vec![(left, now)] } else { vec![] };
        said.or((waiting > 0).then_some(waiting))
    }

    fn spawn(&mut self, cell: (i32, i32), f: Floater, now: f64) {
        let stacked = self.alive.iter().filter(|a| a.cell == cell && now - a.born < STACK).count();
        // a cheap spread for the drift, as Math.random() spreads it
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let r = (self.seed >> 8) as f32 / (1u32 << 24) as f32;
        self.alive.push(Alive { f, cell, lift: stacked as f32 * STACK_PX, born: now, dx: (r * 2.0 - 1.0) * DRIFT_PX });
        while self.alive.len() > MAX_ALIVE {
            self.alive.remove(0);
        }
    }

    /// Whether a number is up (the frame loop runs while one is).
    pub fn alive(&self) -> bool {
        !self.alive.is_empty() || !self.waiting.is_empty()
    }

    /// Draw the numbers: each over its cell where `screen_of` puts it, popping, rising and
    /// fading. `u` is the `--hu` unit.
    pub fn paint(&mut self, painter: &Painter, now: f64, u: f32, screen_of: impl Fn(i32, i32) -> Option<Pos2>) {
        let waiting = std::mem::take(&mut self.waiting);
        for (amount, at, t) in waiting {
            if !self.show_taken(amount, at, now) && now - t < DROP_WAIT {
                self.waiting.push((amount, at, t));
            }
        }
        self.alive.retain(|a| now - a.born < LIFE);
        for a in &self.alive {
            let Some(p) = screen_of(a.cell.0, a.cell.1) else { continue };
            paint_floater(painter, a, p, ((now - a.born) / LIFE) as f32, now - a.born, u);
        }
        if self.alive() {
            painter.ctx().request_repaint();
        }
    }
}

fn tone_colour(t: Tone) -> C {
    match t {
        Tone::Dealt => hex(0xffe9a8),
        Tone::Taken => hex(0xff4a3d),
        Tone::Miss => hex(0xa8a49c),
        Tone::Kill => hex(0xffb347),
        Tone::Heal => hex(0x7fe07a),
    }
}

/// `damage-float`'s keyframes at `t` (0..1): opacity, offset and scale, each segment eased by
/// `cubic-bezier(0.2, 0.7, 0.3, 1)` as the animation's timing function eases each.
fn float_at(t: f32, dx: f32, u: f32) -> (f32, egui::Vec2, f32) {
    let ease = |x: f32| crate::dock::cubic_bezier((0.2, 0.7, 0.3, 1.0), x.clamp(0.0, 1.0));
    let seg = |a: f32, b: f32| ease((t - a) / (b - a));
    let opacity = if t < 0.12 {
        seg(0.0, 0.12)
    } else if t < 0.7 {
        1.0
    } else {
        1.0 - seg(0.7, 1.0)
    };
    let keys = [(0.0, 0.0, 0.0, 0.3), (0.12, dx * 0.2, -6.0 * u, 1.3), (0.26, dx * 0.35, -12.0 * u, 1.0), (1.0, dx, -56.0 * u, 0.85)];
    for w in keys.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t <= b.0 {
            let k = seg(a.0, b.0);
            return (opacity, egui::vec2(a.1 + (b.1 - a.1) * k, a.2 + (b.2 - a.2) * k), a.3 + (b.3 - a.3) * k);
        }
    }
    (opacity, egui::vec2(dx, -56.0 * u), 0.85)
}

/// `damage-shake`: a jolt over its first 0.32 seconds.
fn shake_at(secs: f64, u: f32) -> egui::Vec2 {
    let t = (secs / 0.32) as f32;
    if t >= 1.0 {
        return egui::Vec2::ZERO;
    }
    let keys = [(0.0, 0.0, 0.0), (0.2, -5.0, 2.0), (0.4, 5.0, -2.0), (0.6, -3.0, 1.0), (0.8, 3.0, 0.0), (1.0, 0.0, 0.0)];
    for w in keys.windows(2) {
        if t <= w[1].0 {
            let k = (t - w[0].0) / (w[1].0 - w[0].0);
            return egui::vec2(w[0].1 + (w[1].1 - w[0].1) * k, w[0].2 + (w[1].2 - w[0].2) * k) * u;
        }
    }
    egui::Vec2::ZERO
}

fn paint_floater(painter: &Painter, a: &Alive, at: Pos2, t: f32, secs: f64, u: f32) {
    let (opacity, off, scale) = float_at(t, a.dx, u);
    if opacity <= 0.0 {
        return;
    }
    let f = &a.f;
    let size = 30.0 * u * f.scale * scale;
    let face = if f.tone == Tone::Miss { Face::Caps600 } else { Face::Caps900 };
    let mut fmt = egui::TextFormat { font_id: style::font(face, size.max(1.0)), color: Color32::WHITE, italics: f.tone == Tone::Miss, ..Default::default() };
    if f.tone == Tone::Kill {
        fmt.extra_letter_spacing = 0.12 * size;
    }
    let mut job = egui::text::LayoutJob::default();
    job.append(&f.text, 0.0, fmt);
    let g = painter.ctx().fonts_mut(|fo| fo.layout_job(job));
    // its foot `bottom` above the anchor (44 for a kill), centred on it, then moved and scaled
    // about its own middle
    let bottom = if f.tone == Tone::Kill { 44.0 } else { 14.0 } * u;
    let unscaled_h = g.size().y / scale.max(0.01);
    let centre = at + egui::vec2(0.0, -bottom - unscaled_h / 2.0) + off + if f.shake { shake_at(secs, u) } else { egui::Vec2::ZERO } - egui::vec2(0.0, a.lift);
    let pos = centre - g.size() / 2.0;
    let colour = tone_colour(f.tone);
    // `text-shadow: 0 2 4 rgba(0,0,0,.8)` under a 1.5-unit stroke of #120c06, then the fill
    painter.galley_with_override_text_color(pos + egui::vec2(0.0, 2.0 * u), g.clone(), Color32::BLACK.gamma_multiply(0.5 * opacity));
    let sw = 1.5 * u * 0.75;
    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (0.7, 0.7), (-0.7, 0.7), (0.7, -0.7), (-0.7, -0.7)] {
        painter.galley_with_override_text_color(pos + egui::vec2(dx * sw, dy * sw), g.clone(), hex(0x120c06).alpha(opacity).to_color32());
    }
    painter.galley_with_override_text_color(pos, g, colour.alpha(opacity).to_color32());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floaters() {
        assert_eq!(dealt_floater(1, None), None);
        assert_eq!(dealt_floater(2, None).unwrap().text, "!!");
        assert_eq!(dealt_floater(5, Some(12)).unwrap(), Floater { text: "12".into(), tone: Tone::Dealt, scale: 1.7, shake: true });
        assert_eq!(dealt_floater(0, Some(0)).unwrap().tone, Tone::Miss);
        assert_eq!(taken_floater(8, 20).scale, 2.0);
        assert_eq!(taken_floater(1, 20).scale, 1.0);
    }

    #[test]
    fn a_blow_on_the_player_takes_the_drop() {
        let mut d = DamageNumbers::default();
        d.health(20, 20, (0, 0), 0.0, true);
        d.health(14, 20, (0, 0), 0.1, true);
        d.landing(Landing::Hit { on_player: true, tier: 1, amount: None }, (0, 0), 0.2, true);
        assert_eq!(d.alive.len(), 1);
        assert_eq!(d.alive[0].f.text, "-6");
        // a blow whose line says how much shows that at once
        d.landing(Landing::Hit { on_player: true, tier: 1, amount: Some(3) }, (0, 0), 0.3, true);
        assert_eq!(d.alive[1].f.text, "-3");
        d.health(11, 20, (0, 0), 0.35, true);
        // a blow the line says did nothing shows "0", never "-0"
        d.landing(Landing::Hit { on_player: true, tier: 1, amount: Some(0) }, (0, 0), 0.32, true);
        assert_eq!(d.alive.last().unwrap().f.text, "0");
        // a blow with no amount and no drop yet waits a moment for one
        d.landing(Landing::Hit { on_player: true, tier: 1, amount: None }, (0, 0), 0.36, true);
        assert_eq!(d.waiting.len(), 0);
        // healing shows at once
        d.health(19, 20, (0, 0), 0.5, true);
        assert_eq!(d.alive.last().unwrap().f.text, "+8");
    }

    #[test]
    fn the_float_starts_small_and_ends_faded() {
        let (o, _, s) = float_at(0.0, 10.0, 1.0);
        assert!(o < 0.01 && (s - 0.3).abs() < 1e-3);
        let (o, off, s) = float_at(1.0, 10.0, 1.0);
        assert!(o < 0.01 && (s - 0.85).abs() < 1e-3 && (off.y + 56.0).abs() < 1e-3);
    }
}
