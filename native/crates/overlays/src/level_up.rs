//! The level-up banner (level-up.ts, level-up.css): "You have reached LEVEL N" over the view with
//! the gains read off the `player` message, which fades on its own, and a key or a click dismisses
//! (one that answers a --more-- lets it stay a moment for the gains); and crawl's stat-gain prompt
//! drawn as three cards on the same screen.

use crate::focus::{Act, Focusable};
use crate::frame::Screen;
use crate::prompt::PromptOption;
use crate::rich::{self, TextStyle};
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Vec2};
use isorun_ui::tiles::TileIcons;
use isorun_ui::widgets::{self, rgba, KeycapLook};
use isorun_webtiles::{cm, formatted_string_to_text, GameMessage, PlayerState};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatKey {
    Str,
    Int,
    Dex,
}

struct Stat {
    key: StatKey,
    hotkey: &'static str,
    name: &'static str,
    effect: &'static str,
    icon: &'static str,
    accent: (u8, u8, u8),
}

const STATS: [Stat; 3] = [
    Stat { key: StatKey::Str, hotkey: "S", name: "Strength", effect: "Melee damage, and less armour encumbrance", icon: "FIGHTING_ON", accent: (255, 120, 90) },
    Stat { key: StatKey::Int, hotkey: "I", name: "Intelligence", effect: "Spell power and casting success", icon: "SPELLCASTING_ON", accent: (120, 170, 255) },
    Stat { key: StatKey::Dex, hotkey: "D", name: "Dexterity", effect: "Accuracy, evasion and stealth", icon: "DODGING_ON", accent: (130, 220, 140) },
];

static REACHED_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^You have reached level (\d+)").unwrap());
static DRAMATIC_GAIN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Your experience leads to a dramatic increase").unwrap());
const BANNER: f64 = 2.5;
const LEAVE: f64 = 0.45;
const LINGER: f64 = 1.8;
const ORDINARY_STAT_GAIN: i64 = 2;
const DRAMATIC_STAT_GAIN: i64 = 4;
const RISE_RECENT: f64 = 3.0;
const MORE_ANSWER: f64 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Snapshot {
    xl: i64,
    hp_max: i64,
    mp_max: i64,
    str: i64,
    int: i64,
    dex: i64,
}

impl Snapshot {
    fn of(p: &PlayerState) -> Self {
        Snapshot { xl: p.xl, hp_max: p.hp_max, mp_max: p.mp_max, str: p.str, int: p.int, dex: p.dex }
    }
    fn stat(&self, k: StatKey) -> i64 {
        match k {
            StatKey::Str => self.str,
            StatKey::Int => self.int,
            StatKey::Dex => self.dex,
        }
    }
}

/// The level a message line announces.
pub fn reached_level(text: &str) -> Option<i64> {
    REACHED_RE.captures(text).and_then(|c| c[1].parse().ok())
}

/// What the level brought: `+7 Max HP`, `+1 Max MP`, `+2 Strength`, with the tone of each.
pub fn gains(from: &Snapshot, to: &Snapshot) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    let mut add = |d: i64, label: &str, tone: &'static str| {
        if d > 0 {
            out.push((format!("+{d} {label}"), tone));
        }
    };
    add(to.hp_max - from.hp_max, "Max HP", "hp");
    add(to.mp_max - from.mp_max, "Max MP", "mp");
    for s in &STATS {
        add(to.stat(s.key) - from.stat(s.key), s.name, "stat");
    }
    out
}

/// The banner's state: open or not, its timers, and the stat choice when crawl asks for one.
#[derive(Default)]
pub struct LevelUp {
    open: Option<Banner>,
    baseline: Option<Snapshot>,
    current: Option<Snapshot>,
    rise: Option<(Snapshot, f64)>,
    stat_gain: i64,
    more: bool,
    more_answered_at: f64,
    /// the stat choice is up: its options, and whether the pad spoke last
    choice: Option<(Vec<PromptOption>, bool)>,
}

struct Banner {
    level: i64,
    opened: f64,
    /// when it starts to leave
    leave_at: Option<f64>,
    /// it is leaving, since then
    leaving: Option<f64>,
}

impl LevelUp {
    pub fn choosing(&self) -> bool {
        self.choice.is_some()
    }

    /// Whether the banner is up.
    pub fn shown(&self) -> bool {
        self.open.is_some()
    }

    fn gain(&self) -> i64 {
        if self.stat_gain == 0 {
            ORDINARY_STAT_GAIN
        } else {
            self.stat_gain
        }
    }

    /// The message lines that arrived.
    pub fn messages(&mut self, lines: &[GameMessage], now: f64) {
        for l in lines {
            let text = formatted_string_to_text(&l.text);
            let text = text.trim();
            if let Some(level) = reached_level(text) {
                self.celebrate(level, now);
            }
            if DRAMATIC_GAIN_RE.is_match(text) {
                self.stat_gain = DRAMATIC_STAT_GAIN;
            }
        }
    }

    /// The player's numbers, and whether a --more-- is up, every frame.
    pub fn sync(&mut self, p: &PlayerState, more: bool, now: f64) {
        let snap = Snapshot::of(p);
        let before = self.current;
        if before != Some(snap) {
            self.current = Some(snap);
            if let Some(b) = before {
                if p.xl > b.xl {
                    self.rise = Some((b, now));
                }
            }
        }
        let answered = self.more && !more;
        if answered {
            self.more_answered_at = now;
        }
        self.more = more;
        // the timers
        if let Some(b) = &mut self.open {
            if answered && self.choice.is_none() && b.leaving.is_none() {
                b.leave_at = Some(now + LINGER);
            }
            if b.leaving.is_none() && self.choice.is_none() && b.leave_at.is_some_and(|t| now >= t) {
                b.leaving = Some(now);
            }
            if b.leaving.is_some_and(|t| now >= t + LEAVE) {
                self.close();
            }
        }
    }

    fn open_banner(&mut self, level: i64, now: f64) {
        let rise = self.rise.filter(|r| now - r.1 < RISE_RECENT).map(|r| r.0);
        self.baseline = rise.or(self.current);
        self.rise = None;
        self.open = Some(Banner { level, opened: now, leave_at: None, leaving: None });
    }

    fn celebrate(&mut self, level: i64, now: f64) {
        if self.open.is_none() {
            self.open_banner(level, now);
        }
        let choosing = self.choice.is_some();
        let b = self.open.as_mut().unwrap();
        b.level = level;
        b.leaving = None;
        b.leave_at = (!choosing).then_some(now + BANNER);
    }

    /// Crawl asks for the stat: the cards are the prompt's stops.
    pub fn choose(&mut self, options: &[PromptOption], pad: bool, now: f64) -> Vec<Focusable> {
        if self.open.is_none() {
            self.open_banner(self.current.map_or(0, |c| c.xl) + 1, now);
        }
        if let Some(b) = self.open.as_mut() {
            b.leave_at = None;
            b.leaving = None;
        }
        self.choice = Some((options.to_vec(), pad));
        STATS
            .iter()
            .enumerate()
            .filter_map(|(col, s)| {
                let o = options.iter().find(|o| o.hotkey.to_ascii_uppercase() == s.hotkey)?;
                Some(Focusable { label: s.name.into(), act: Act::send([cm::input(&o.hotkey)]), row: Some(0), col: Some(col as i32), id: Some(format!("stat:{}", s.hotkey)), ..Default::default() })
            })
            .collect()
    }

    pub fn end_choice(&mut self, now: f64) {
        if self.choice.take().is_none() {
            return;
        }
        self.stat_gain = 0;
        if let Some(b) = self.open.as_mut() {
            b.leaving = None;
            b.leave_at = Some(now + BANNER);
        }
    }

    fn close(&mut self) {
        self.open = None;
        self.baseline = None;
    }

    fn dismiss(&mut self, now: f64) {
        if self.choice.is_some() {
            return;
        }
        if let Some(b) = self.open.as_mut() {
            if b.leaving.is_none() {
                b.leaving = Some(now);
            }
        }
    }

    fn linger(&mut self, now: f64) {
        if let Some(b) = self.open.as_mut() {
            b.leaving = None;
            b.leave_at = Some(now + LINGER);
        }
    }

    /// A key or a click off the panel: it goes, or stays a moment when it answered a --more--.
    pub fn answer(&mut self, now: f64) {
        if self.open.is_none() || self.choice.is_some() {
            return;
        }
        if self.more || now - self.more_answered_at < MORE_ANSWER {
            self.linger(now);
        } else {
            self.dismiss(now);
        }
    }

    /// Draw the banner (and the cards); the messages a click sends go to `out`. `lit` is the stop
    /// the cursor is on. Returns true while it is up.
    pub fn draw(&mut self, ctx: &egui::Context, screen: &Screen, icons: &mut TileIcons, lit: Option<&str>, out: &mut Vec<serde_json::Value>, now: f64) -> bool {
        if self.open.is_some() && self.baseline.is_none() {
            self.baseline = self.current;
        }
        let Some(b) = &self.open else { return false };
        // the panel's height at a rem: a short window (a phone on its side) sets the whole panel
        // smaller until it fits, the number as touch.css sets it (3.2rem under 560px)
        let short = screen.h() < 560.0;
        let level_size_at = |rem: f32| if short { 3.2 * rem } else { (0.11 * screen.w()).clamp(4.5 * rem, 7.5 * rem) };
        let choosing_now = self.choice.is_some();
        let height_at = |rem: f32| {
            let narrow = screen.w() <= 760.0;
            let pad = if narrow { (rem, rem, rem) } else { (1.5 * rem, 3.0 * rem, 1.8 * rem) };
            let card_h = 1.2 * rem + 5.2 * rem + 0.45 * rem + 1.7 * rem + 0.45 * rem + 1.35 * rem * 1.2 + 0.45 * rem + 2.6 * 0.85 * rem + 0.45 * rem + 1.7 * rem + 1.0 * rem;
            let mut h = pad.0 + 0.95 * rem * 1.3 + level_size_at(rem) * 1.1 * 0.82 + 0.5 * rem + 2.0 + 0.9 * rem + 1.9 * rem + pad.2;
            if choosing_now {
                h += 0.4 * rem + 1.6 * rem * 1.3 + rem + card_h + 1.2 * rem + 1.7 * 0.9 * rem;
            }
            h
        };
        let base = screen.rem();
        let room = 0.96 * screen.h();
        let rem = if height_at(base) > room { (base * room / height_at(base)).max(0.6 * base) } else { base };
        let fade = match b.leaving {
            Some(t) => (1.0 - (now - t) / LEAVE).clamp(0.0, 1.0) as f32,
            None => ((now - b.opened) / 0.3).clamp(0.0, 1.0) as f32,
        };
        let lift = b.leaving.map_or(0.0, |t| ((now - t) / LEAVE).clamp(0.0, 1.0) as f32 * 14.0);
        let level = b.level;
        let alpha = |c: Color32| c.gamma_multiply(fade);
        let display = FontFamily::Name("title".into());
        // the panel's parts
        let kicker_font = FontId::new(0.95 * rem, isorun_ui::fonts::bold());
        let kicker = spaced(ctx, "YOU HAVE REACHED", kicker_font, 0.6 * 0.95 * rem, rgba(243, 214, 140, 1.0));
        let level_size = level_size_at(rem);
        let level_g = ctx.fonts_mut(|f| f.layout_no_wrap(format!("LEVEL {level}"), FontId::new(level_size, display.clone()), Color32::WHITE));
        let gains = match (self.baseline, self.current) {
            (Some(a), Some(c)) => gains(&a, &c),
            _ => Vec::new(),
        };
        let gain_st = TextStyle::new(ctx, rem, Color32::WHITE).bold();
        let gain_gs: Vec<_> = gains
            .iter()
            .map(|(l, tone)| {
                let c = match *tone {
                    "hp" => widgets::hex(0xff8a7a),
                    "mp" => widgets::hex(0x8ab8ff),
                    _ => widgets::hex(0xf3d68c),
                };
                (rich::galley(ctx, rich::plain(l, &gain_st.clone().colour(c), 0.0)), *tone)
            })
            .collect();
        let choosing = self.choice.clone();
        let card_w = if screen.w() <= 760.0 { ((screen.w() - 2.0 * rem - 1.2 * rem) / 3.0).max(60.0) } else { 14.5 * rem };
        let card_gap = if screen.w() <= 760.0 { 0.6 * rem } else { 1.1 * rem };
        let ask_g = choosing.as_ref().map(|_| ctx.fonts_mut(|f| f.layout_no_wrap(format!("Raise an attribute by {}", self.gain()), FontId::new(1.6 * rem, display.clone()), widgets::hex(0xefe4cc))));
        let card_h = 1.2 * rem + 5.2 * rem + 0.45 * rem + 1.7 * rem + 0.45 * rem + 1.35 * rem * 1.2 + 0.45 * rem + 2.6 * 0.85 * rem + 0.45 * rem + 1.7 * rem + 1.0 * rem;
        let gains_h = 1.9 * rem;
        let pad = if screen.w() <= 760.0 { (rem, rem, rem) } else { (1.5 * rem, 3.0 * rem, 1.8 * rem) };
        let mut h = pad.0 + kicker.size().y + level_g.size().y * 0.82 + 0.5 * rem + 2.0 + 0.9 * rem + gains_h + pad.2;
        if choosing.is_some() {
            h += 0.4 * rem + ask_g.as_ref().map_or(0.0, |g| g.size().y) + rem + card_h + 1.2 * rem + 1.7 * 0.9 * rem;
        }
        let cards_w = 3.0 * card_w + 2.0 * card_gap;
        let w = (22.0 * rem).max(if choosing.is_some() { cards_w } else { 0.0 }) + 2.0 * pad.1;
        // centred in the screen less its 8vh foot
        let area = if h > screen.h() * 0.84 { screen.rect } else { Rect::from_min_max(screen.rect.min, Pos2::new(screen.rect.right(), screen.rect.bottom() - 0.08 * screen.h())) };
        let panel = Rect::from_center_size(area.center() - Vec2::new(0.0, lift), Vec2::new(w, h));
        let mut clicked: Option<String> = None;
        let mut panel_clicked = false;
        let mut outside = false;
        let more = self.more;
        let backdrop = egui::Area::new(egui::Id::unique("isorun-level-up-dim")).order(egui::Order::Foreground).fixed_pos(screen.rect.min).interactable(false).show(ctx, |ui| {
            let p = ui.painter();
            let stops = if choosing.is_some() { vec![(0.0, rgba(4, 3, 2, 0.75)), (0.75, rgba(4, 3, 2, 0.45)), (1.0, rgba(4, 3, 2, 0.45))] } else { vec![(0.0, rgba(4, 3, 2, 0.6)), (0.6, rgba(4, 3, 2, 0.0))] };
            let stops: Vec<_> = stops.into_iter().map(|(t, c)| (t, alpha(c))).collect();
            let tex = widgets::radial_gradient(ui.ctx(), (0.5, 0.45), screen.w() / screen.h(), &stops);
            widgets::paint_texture(p, screen.rect, 0.0, tex);
        });
        ctx.move_to_top(backdrop.response.layer_id);
        // a click anywhere off the panel answers it
        if ctx.input(|i| i.pointer.any_pressed()) {
            if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                outside = !panel.contains(pos);
            }
        }
        crate::frame::layer(ctx, egui::Id::unique("isorun-level-up"), panel, |ui, resp| {
            let p = ui.painter().clone();
            let cx = panel.center().x;
            let mut y = panel.top() + pad.0;
            // the rays and the glow behind the number
            let glow_c = Pos2::new(cx, panel.top() + 4.6 * rem);
            let glow = widgets::radial_gradient(ui.ctx(), (0.5, 0.5), 26.0 / 14.0, &[(0.0, alpha(rgba(255, 205, 110, 0.55 * 0.6))), (0.45, alpha(rgba(255, 170, 60, 0.18 * 0.6))), (0.7, Color32::TRANSPARENT)]);
            widgets::paint_texture(&p, Rect::from_center_size(glow_c, Vec2::new(26.0 * rem, 14.0 * rem)), 0.0, glow);
            rays(&p, glow_c, 23.0 * rem, ((now - b_opened(&self.open)) as f32).min(2.6), fade);
            p.galley(Pos2::new(cx - kicker.size().x / 2.0 + 0.3 * 0.95 * rem, y), kicker.clone(), alpha(rgba(243, 214, 140, 1.0)));
            y += kicker.size().y;
            gradient_text(&p, &level_g, Pos2::new(cx - level_g.size().x / 2.0, y - level_g.size().y * 0.09), fade);
            y += level_g.size().y * 0.82 + 0.5 * rem;
            // the rule fades in and out at its ends, with a glow (`box-shadow: 0 0 10px`)
            let rule_r = Rect::from_center_size(Pos2::new(cx, y + 1.0), Vec2::new(22.0 * rem, 2.0));
            p.add(egui::epaint::Shadow { offset: [0, 0], blur: 10, spread: 0, color: rgba(255, 200, 100, 0.35 * fade) }.as_shape(rule_r.shrink2(Vec2::new(4.0 * rem, 0.0)), egui::CornerRadius::ZERO));
            widgets::hgradient(&p, rule_r, &[(0.0, Color32::TRANSPARENT), (0.3, alpha(rgba(243, 214, 140, 1.0))), (0.5, alpha(widgets::hex(0xfff3cf))), (0.7, alpha(rgba(243, 214, 140, 1.0))), (1.0, Color32::TRANSPARENT)]);
            y += 2.0 + 0.9 * rem;
            // the gains, chips side by side
            let gw: f32 = gain_gs.iter().map(|(g, _)| g.size().x + 1.5 * rem + 2.0).sum::<f32>() + gain_gs.len().saturating_sub(1) as f32 * 0.6 * rem;
            let mut x = cx - gw / 2.0;
            for (g, tone) in &gain_gs {
                let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(g.size().x + 1.5 * rem + 2.0, g.size().y + 0.5 * rem + 2.0));
                let border = match *tone {
                    "hp" => rgba(255, 110, 90, 0.45),
                    "mp" => rgba(110, 160, 255, 0.45),
                    _ => rgba(243, 214, 140, 0.35),
                };
                p.rect_filled(r, r.height() / 2.0, alpha(rgba(12, 10, 8, 0.82)));
                p.rect_stroke(r, r.height() / 2.0, egui::Stroke::new(1.0, alpha(border)), egui::StrokeKind::Inside);
                p.galley(r.center() - g.size() / 2.0, g.clone(), Color32::WHITE);
                x = r.right() + 0.6 * rem;
            }
            y += gains_h;
            if let (Some((options, pad_kind)), Some(ask)) = (&choosing, &ask_g) {
                y += 0.4 * rem;
                p.galley(Pos2::new(cx - ask.size().x / 2.0, y), ask.clone(), widgets::hex(0xefe4cc));
                y += ask.size().y + rem;
                let mut x = cx - cards_w / 2.0;
                for s in &STATS {
                    let Some(o) = options.iter().find(|o| o.hotkey.to_ascii_uppercase() == s.hotkey) else { continue };
                    let id = format!("stat:{}", s.hotkey);
                    let focused = lit == Some(id.as_str());
                    let r = Rect::from_min_size(Pos2::new(x, y - if focused { 6.0 } else { 0.0 }), Vec2::new(card_w, card_h));
                    let resp = ui.interact(r, egui::Id::unique(("isorun-stat", s.hotkey)), egui::Sense::click());
                    stat_card(ui, r, s, self.current.map_or(0, |c| c.stat(s.key)), self.gain(), focused, resp.hovered(), *pad_kind, icons, rem);
                    if resp.clicked() {
                        clicked = Some(o.hotkey.clone());
                    }
                    x += card_w + card_gap;
                }
                y += card_h + 1.2 * rem;
                // the footer: how to pick, for the device that spoke last
                let foot = TextStyle::new(ui.ctx(), 0.9 * rem, widgets::hex(0xa59a87));
                let cap = KeycapLook::level_up(0.85 * rem);
                let parts: Vec<widgets::KeyHint> = if *pad_kind {
                    vec![widgets::KeyHint::Text("✛ Pick  (A) Choose", foot.font(), foot.colour)]
                } else {
                    vec![
                        widgets::KeyHint::Text("Press", foot.font(), foot.colour),
                        widgets::KeyHint::Gap(0.45 * rem),
                        widgets::KeyHint::Cap("S"),
                        widgets::KeyHint::Gap(0.45 * rem),
                        widgets::KeyHint::Cap("I"),
                        widgets::KeyHint::Gap(0.45 * rem),
                        widgets::KeyHint::Cap("D"),
                        widgets::KeyHint::Gap(0.45 * rem),
                        widgets::KeyHint::Text("or the arrows and", foot.font(), foot.colour),
                        widgets::KeyHint::Gap(0.45 * rem),
                        widgets::KeyHint::Cap("Enter"),
                    ]
                };
                let size = widgets::key_hints(&ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::unique("isorun-measure"))).with_clip_rect(Rect::NOTHING), Pos2::ZERO, &parts, &cap);
                widgets::key_hints(&p, Pos2::new(cx - size.width() / 2.0, y), &parts, &cap);
            }
            if resp.clicked() {
                panel_clicked = true;
            }
        });
        if let Some(h) = clicked {
            out.push(cm::input(&h));
        } else if panel_clicked && choosing.is_none() {
            if more {
                out.push(cm::key(32));
            }
            self.answer(now);
        } else if outside {
            self.answer(now);
        }
        ctx.request_repaint();
        true
    }
}

fn b_opened(b: &Option<Banner>) -> f64 {
    b.as_ref().map_or(0.0, |b| b.opened)
}

/// Text laid out with letter spacing (CSS `letter-spacing`), as one galley.
fn spaced(ctx: &egui::Context, text: &str, font: FontId, spacing: f32, colour: Color32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    for (i, c) in text.chars().enumerate() {
        job.append(&c.to_string(), if i == 0 { 0.0 } else { spacing }, egui::TextFormat { font_id: font.clone(), color: colour, ..Default::default() });
    }
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// The number's gold: `linear-gradient(180deg, #fff6d6, #f7d88a 38%, #c98f35 72%, #8a5a1c)` clipped
/// to the text, drawn as bands of the galley, over its dark drop shadow.
fn gradient_text(p: &egui::Painter, g: &std::sync::Arc<egui::Galley>, at: Pos2, fade: f32) {
    let stops = [(0.0, (0xff, 0xf6, 0xd6)), (0.38, (0xf7, 0xd8, 0x8a)), (0.72, (0xc9, 0x8f, 0x35)), (1.0, (0x8a, 0x5a, 0x1c))];
    let colour_at = |t: f32| {
        for w in stops.windows(2) {
            if t <= w[1].0 {
                let k = (t - w[0].0) / (w[1].0 - w[0].0);
                let l = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * k) as u8;
                return Color32::from_rgb(l(w[0].1 .0, w[1].1 .0), l(w[0].1 .1, w[1].1 .1), l(w[0].1 .2, w[1].1 .2));
            }
        }
        Color32::from_rgb(0x8a, 0x5a, 0x1c)
    };
    let size = g.size();
    p.galley_with_override_text_color(at + Vec2::new(0.0, 4.0), g.clone(), rgba(0, 0, 0, 0.9 * fade));
    let glow = rgba(255, 180, 70, 0.25 * fade);
    for d in [Vec2::new(-2.0, 0.0), Vec2::new(2.0, 0.0), Vec2::new(0.0, -2.0), Vec2::new(0.0, 2.0)] {
        p.galley_with_override_text_color(at + d, g.clone(), glow);
    }
    let bands = 24;
    for i in 0..bands {
        let t0 = i as f32 / bands as f32;
        let t1 = (i + 1) as f32 / bands as f32;
        let clip = Rect::from_min_max(Pos2::new(at.x - 4.0, at.y + size.y * t0), Pos2::new(at.x + size.x + 4.0, at.y + size.y * t1));
        p.with_clip_rect(clip.intersect(p.clip_rect())).galley_with_override_text_color(at, g.clone(), colour_at((t0 + t1) / 2.0).gamma_multiply(fade));
    }
}

/// `repeating-conic-gradient` rays turning out from behind the number, masked to a soft disc.
fn rays(p: &egui::Painter, c: Pos2, r: f32, t: f32, fade: f32) {
    let k = (t / 2.6).clamp(0.0, 1.0);
    let ease = 1.0 - (1.0 - k).powi(3);
    let turn = (-25.0 + 45.0 * ease).to_radians();
    let scale = 0.4 + 0.6 * ease;
    let opacity = if k < 0.25 { k / 0.25 } else { 1.0 - 0.55 * (k - 0.25) / 0.75 } * fade;
    let n = 24;
    for i in 0..n {
        let a0 = turn + (i as f32) * std::f32::consts::TAU / n as f32;
        let a1 = a0 + 2.5f32.to_radians();
        let mut mesh = egui::Mesh::default();
        let centre = rgba(255, 220, 140, 0.38 * opacity);
        mesh.colored_vertex(c, centre);
        let steps = 6;
        for s in 0..=steps {
            let f = s as f32 / steps as f32;
            let rr = r * scale * (0.08 + 0.54 * f);
            let alpha = if f < 0.35 { 1.0 - f * 1.3 } else { 0.55 * (1.0 - (f - 0.35) / 0.65) };
            let col = rgba(255, 220, 140, 0.38 * opacity * alpha.max(0.0));
            mesh.colored_vertex(c + Vec2::angled(a0) * rr, col);
            mesh.colored_vertex(c + Vec2::angled(a1) * rr, col);
        }
        for s in 0..steps {
            let b = 1 + 2 * s as u32;
            mesh.add_triangle(0, b, b + 1);
            mesh.add_triangle(b, b + 1, b + 2);
            mesh.add_triangle(b + 1, b + 2, b + 3);
        }
        p.add(egui::Shape::mesh(mesh));
    }
}

/// One stat's card (`.level-up-card`).
#[allow(clippy::too_many_arguments)]
fn stat_card(ui: &mut egui::Ui, r: Rect, s: &Stat, now_val: i64, gain: i64, focused: bool, hovered: bool, pad: bool, icons: &mut TileIcons, rem: f32) {
    let p = ui.painter().clone();
    let (ar, ag, ab) = s.accent;
    let accent = |a: f32| rgba(ar, ag, ab, a);
    let shadow = egui::epaint::Shadow { offset: [0, if focused { 14 } else { 10 }], blur: 30, spread: 0, color: rgba(0, 0, 0, 0.6) };
    p.add(shadow.as_shape(r, egui::CornerRadius::same(10)));
    if focused {
        p.add(egui::epaint::Shadow { offset: [0, 0], blur: 26, spread: 0, color: accent(0.35) }.as_shape(r, egui::CornerRadius::same(10)));
    }
    let ctx = ui.ctx();
    widgets::paint_texture(&p, r, 10.0, widgets::linear_gradient(ctx, 180, &[(0.0, widgets::hex(0x1c1a17)), (1.0, widgets::hex(0x0d0c0b))]));
    widgets::paint_texture(&p, r, 10.0, widgets::radial_gradient(ctx, (0.5, 0.0), r.width() / r.height(), &[(0.0, accent(0.16)), (0.65, accent(0.0))]));
    let border = if focused {
        accent(0.9)
    } else if hovered {
        accent(0.6)
    } else {
        widgets::hex(0x3a3226)
    };
    p.rect_stroke(r, 10.0, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
    if focused {
        p.rect_stroke(r.shrink(1.0), 9.0, egui::Stroke::new(1.0, accent(0.35)), egui::StrokeKind::Inside);
    }
    let cx = r.center().x;
    let mut y = r.top() + 1.2 * rem;
    // the emblem: a ring with the skill's icon
    let er = 2.6 * rem;
    let ec = Pos2::new(cx, y + er);
    p.add(egui::epaint::Shadow { offset: [0, 0], blur: 18, spread: 0, color: accent(0.25) }.as_shape(Rect::from_center_size(ec, Vec2::splat(2.0 * er)), egui::CornerRadius::same(er as u8)));
    widgets::paint_texture(&p, Rect::from_center_size(ec, Vec2::splat(2.0 * er)), er, widgets::radial_gradient(ctx, (0.5, 0.5), 1.0, &[(0.0, accent(0.28)), (0.7, rgba(0, 0, 0, 0.6)), (1.0, rgba(0, 0, 0, 0.6))]));
    p.circle_stroke(ec, er - 1.0, egui::Stroke::new(2.0, accent(0.55)));
    if let Some(tex) = screen_icon(ctx, icons, s.icon) {
        // `mix-blend-mode: screen; clip-path: circle(42%)`: the icon's black is no colour at all, and
        // only the disc of it shows
        let r = Rect::from_center_size(ec, Vec2::splat(64.0));
        let mut mesh = egui::Mesh::with_texture(tex.id());
        mesh.vertices.push(egui::epaint::Vertex { pos: ec, uv: Pos2::new(0.5, 0.5), color: Color32::WHITE });
        let n = 32u32;
        for i in 0..=n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let d = Vec2::angled(a) * 0.42;
            mesh.vertices.push(egui::epaint::Vertex { pos: ec + d * r.width(), uv: Pos2::new(0.5 + d.x, 0.5 + d.y), color: Color32::WHITE });
            if i > 0 {
                mesh.add_triangle(0, i, i + 1);
            }
        }
        p.add(egui::Shape::mesh(mesh));
    }
    y += 2.0 * er + 0.45 * rem;
    let name = ctx.fonts_mut(|f| f.layout_no_wrap(s.name.into(), FontId::new(1.7 * rem, FontFamily::Name("title".into())), accent(1.0)));
    p.galley(Pos2::new(cx - name.size().x / 2.0, y), name.clone(), accent(1.0));
    y += 1.7 * rem + 0.45 * rem;
    let vf = FontId::new(1.35 * rem, isorun_ui::fonts::bold());
    let from = ctx.fonts_mut(|f| f.layout_no_wrap(now_val.to_string(), vf.clone(), widgets::hex(0xa59a87)));
    let arrow = ctx.fonts_mut(|f| f.layout_no_wrap("→".into(), FontId::new(rem, isorun_ui::fonts::bold()), widgets::hex(0x6f6555)));
    let to = ctx.fonts_mut(|f| f.layout_no_wrap((now_val + gain).to_string(), vf, Color32::WHITE));
    let total = from.size().x + arrow.size().x + to.size().x + rem;
    let mut x = cx - total / 2.0;
    let base = y + from.size().y;
    p.galley(Pos2::new(x, base - from.size().y), from.clone(), widgets::hex(0xa59a87));
    x += from.size().x + 0.5 * rem;
    p.galley(Pos2::new(x, base - arrow.size().y - 2.0), arrow.clone(), widgets::hex(0x6f6555));
    x += arrow.size().x + 0.5 * rem;
    p.galley(Pos2::new(x, base - to.size().y), to.clone(), Color32::WHITE);
    y += 1.35 * rem * 1.2 + 0.45 * rem;
    let eff = rich::galley(ctx, rich::plain(s.effect, &TextStyle::new(ctx, 0.85 * rem, widgets::hex(0xb9ae9a)).line(1.3 * 0.85 * rem), r.width() - 2.0 * rem));
    p.galley(Pos2::new(cx - eff.size().x / 2.0, y), eff, widgets::hex(0xb9ae9a));
    if !pad {
        let cap = KeycapLook::level_up(rem);
        let size = widgets::keycap_size(ctx, s.hotkey, &cap);
        widgets::keycap(&p, Pos2::new(cx - size.x / 2.0, r.bottom() - rem - size.y), s.hotkey, &cap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_level_and_its_gains() {
        let mut lu = LevelUp::default();
        let player = PlayerState { xl: 1, hp_max: 19, mp_max: 0, str: 20, int: 5, dex: 9, ..Default::default() };
        lu.sync(&player, false, 0.0);
        lu.messages(&[GameMessage { text: "You have reached level 2!".into(), channel: Some(11), turn: Some(7), ..Default::default() }], 0.1);
        lu.sync(&PlayerState { xl: 2, hp_max: 26, mp_max: 1, ..player.clone() }, false, 0.2);
        assert_eq!(lu.open.as_ref().unwrap().level, 2);
        let g: Vec<String> = gains(&lu.baseline.unwrap(), &lu.current.unwrap()).into_iter().map(|g| g.0).collect();
        assert_eq!(g, ["+7 Max HP", "+1 Max MP"]);
        // it fades on its own
        lu.sync(&PlayerState { xl: 2, hp_max: 26, mp_max: 1, ..player.clone() }, false, 3.0);
        lu.sync(&PlayerState { xl: 2, hp_max: 26, mp_max: 1, ..player }, false, 3.6);
        assert!(!lu.shown());
    }

    #[test]
    fn a_key_after_a_more_lets_it_linger() {
        let mut lu = LevelUp::default();
        let p = PlayerState::default();
        lu.sync(&p, true, 0.0);
        lu.messages(&[GameMessage { text: "You have reached level 3!".into(), ..Default::default() }], 0.0);
        lu.sync(&p, false, 0.5);
        lu.answer(0.6);
        lu.sync(&p, false, 1.0);
        assert!(lu.shown());
        // a key long after goes at once (after its fade)
        lu.answer(2.5);
        lu.sync(&p, false, 3.0);
        assert!(!lu.shown());
    }

    #[test]
    fn the_stat_choice() {
        let mut lu = LevelUp::default();
        let opts = vec![PromptOption::new("S", "(S)trength"), PromptOption::new("I", "(I)ntelligence"), PromptOption::new("D", "(D)exterity")];
        let f = lu.choose(&opts, false, 0.0);
        assert_eq!(f.iter().map(|f| f.label.as_str()).collect::<Vec<_>>(), ["Strength", "Intelligence", "Dexterity"]);
        assert!(lu.shown() && lu.choosing());
        lu.answer(0.1);
        lu.sync(&PlayerState::default(), false, 5.0);
        assert!(lu.shown());
        lu.end_choice(5.0);
        assert!(!lu.choosing());
    }
}

/// A gui tile as a texture of its own whose black is transparent (the CSS `screen` blend over the
/// dark emblem), made once per tile.
fn screen_icon(ctx: &egui::Context, icons: &TileIcons, name: &str) -> Option<egui::TextureHandle> {
    let key = egui::Id::unique(("isorun-screen-icon", name));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return Some(t);
    }
    let gd = icons.gamedata()?;
    let id = gd.gd.gui.id(name)?;
    let rect = gd.gd.tile(id, None)?;
    let img = gd.atlases.get(&rect.atlas)?;
    let (w, h) = (rect.cell.max(32) as usize, rect.cell.max(32) as usize);
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..rect.h as usize {
        for x in 0..rect.w as usize {
            let (dx, dy) = (x as i64 + rect.ox as i64, y as i64 + rect.oy as i64);
            if dx < 0 || dy < 0 || dx >= w as i64 || dy >= h as i64 {
                continue;
            }
            let si = (((rect.sy as usize + y) * img.width as usize) + rect.sx as usize + x) * 4;
            let di = (dy as usize * w + dx as usize) * 4;
            let px = &img.rgba[si..si + 4];
            let a = px[0].max(px[1]).max(px[2]) as u32 * px[3] as u32 / 255;
            // un-premultiply by the brightness: screen over black keeps the colour, over grey adds it
            for c in 0..3 {
                rgba[di + c] = if a > 0 { ((px[c] as u32 * 255) / a.max(1)).min(255) as u8 } else { 0 };
            }
            rgba[di + 3] = a as u8;
        }
    }
    let t = ctx.load_texture(format!("screen-icon:{name}"), egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba), egui::TextureOptions::NEAREST);
    ctx.data_mut(|d| d.insert_temp(key, t.clone()));
    Some(t)
}
