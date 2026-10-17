//! The dock: the HUD along the bottom of the screen, as an action RPG lays it out (dock.ts,
//! dock.css, and touch.css for the compact dock). A health orb on the left and a magic orb on the
//! right; between them the quick slots, the experience bar, the buttons that open every screen
//! (inventory, character, spells, abilities, skills, the level map, the menu), the defences and
//! attributes, the place and the gold; over it the actions the square offers and the status
//! lights.
//!
//! Everything is a button for the mouse, and the pad walks the same buttons once L3 puts its
//! cursor on the dock (`focus_on`). A window under 900 × 560 gets the compact dock: bars in place
//! of the orbs, the slots and the buttons in a grid along the foot, the actions in a row over it
//! with Explore or Autofight a larger button at the right end.

use crate::dock_actions::{DockAction, DockIcon};
use crate::glyphs::{glyph_size, paint_glyph, Glyph, PadKind};
use crate::icons::{self, Placed};
use crate::paint::{hex, inset, outer, rgba, s, sa, sp, At, Border, BoxCache, BoxStyle, Layer, RadialSize, Stop, C, TRANSPARENT};
use crate::quick_slots::{QuickSlot, SlotRef, SLOT_COUNT};
use crate::style::{self, Face, TextStyle, BETTER, BRONZE, BRONZE_BRIGHT, BRONZE_DIM, GOLD_BRIGHT, IRON, IRON_DEEP, IRON_LINE, WORSE};
use crate::tooltip::{keycaps_width, layout_card, paint_keycaps, CardKind, KeyHint, KeycapSize, Tip};
use crate::{Action, Device};
use egui::{Color32, Painter, Pos2, Rect, Sense, Vec2};
use isorun_ui::tiles::TileIcons;
use isorun_webtiles::{formatted_string_to_text, StatusEntry};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockButtonId {
    Inventory,
    Character,
    Spells,
    Abilities,
    Skills,
    Map,
    Menu,
}

struct ButtonDef {
    id: DockButtonId,
    label: &'static str,
    about: &'static str,
    icon: &'static str,
    keys: &'static [&'static str],
    /// the pad button that opens the same screen, worn as a badge while the pad is in use
    pad: Option<Glyph>,
}

const BUTTONS: [ButtonDef; 7] = [
    ButtonDef { id: DockButtonId::Inventory, label: "Inventory", about: "What you wear and carry", icon: "CMD_DISPLAY_INVENTORY", keys: &["i"], pad: Some(Glyph::Y) },
    ButtonDef { id: DockButtonId::Character, label: "Character", about: "Your defences, attributes and status", icon: "CMD_DISPLAY_CHARACTER_STATUS", keys: &["@"], pad: None },
    ButtonDef { id: DockButtonId::Spells, label: "Spells", about: "Cast a spell you know", icon: "CMD_CAST_SPELL", keys: &["z"], pad: Some(Glyph::X) },
    ButtonDef { id: DockButtonId::Abilities, label: "Abilities", about: "Use an ability", icon: "CMD_USE_ABILITY", keys: &["a"], pad: None },
    ButtonDef { id: DockButtonId::Skills, label: "Skills", about: "What you train, and how fast", icon: "CMD_DISPLAY_SKILLS", keys: &["m"], pad: None },
    ButtonDef { id: DockButtonId::Map, label: "Map", about: "The level map, and travel", icon: "CMD_DISPLAY_MAP", keys: &["X"], pad: Some(Glyph::Select) },
    ButtonDef { id: DockButtonId::Menu, label: "Menu", about: "Settings, help, save and exit", icon: "CMD_GAME_MENU", keys: &["Esc"], pad: Some(Glyph::Start) },
];

/// The icons' sizes in units: the buttons', the actions', the main action's, the slots'.
#[derive(Clone, Copy)]
struct IconSizes {
    button: f32,
    action: f32,
    main: f32,
    slot: f32,
}
const ICON: IconSizes = IconSizes { button: 40.0, action: 44.0, main: 44.0, slot: 48.0 };
const COMPACT_ICON: IconSizes = IconSizes { button: 30.0, action: 30.0, main: 44.0, slot: 28.0 };

fn is_main(id: &str) -> bool {
    id == "fight" || id == "explore"
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct DockStats {
    pub ac: i64,
    pub ev: i64,
    pub sh: i64,
    pub ac_mod: i64,
    pub ev_mod: i64,
    pub sh_mod: i64,
    pub str: i64,
    pub int: i64,
    pub dex: i64,
    pub str_max: i64,
    pub int_max: i64,
    pub dex_max: i64,
}

/// What the dock shows: the game screen builds it from the `player` message (`DockView::from`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DockView {
    pub hp: i64,
    pub hp_max: i64,
    /// the hit points poison will leave (`poison_survival`), drawn as a darker band in the orb
    pub poison_survival: i64,
    pub mp: i64,
    pub mp_max: i64,
    pub xl: i64,
    pub progress: i64,
    pub gold: i64,
    pub place: String,
    pub stats: DockStats,
    pub statuses: Vec<StatusEntry>,
    pub actions: Vec<DockAction>,
    pub slots: Vec<Option<QuickSlot>>,
    /// a spectator watches: the orbs and the bar only, nothing to press
    pub spectating: bool,
    pub device: Device,
    pub pad: PadKind,
}

impl DockView {
    /// The `player` message's numbers (game.ts `updateDock`); the actions and slots come apart.
    pub fn from_player(p: &isorun_webtiles::PlayerState) -> DockView {
        DockView {
            hp: p.hp,
            hp_max: p.hp_max,
            poison_survival: p.poison_survival,
            mp: p.mp,
            mp_max: p.mp_max,
            xl: p.xl,
            progress: p.progress,
            gold: p.gold,
            place: if p.depth != 0 { format!("{}:{}", p.place, p.depth) } else { p.place.clone() },
            stats: DockStats { ac: p.ac, ev: p.ev, sh: p.sh, ac_mod: p.ac_mod, ev_mod: p.ev_mod, sh_mod: p.sh_mod, str: p.str, int: p.int, dex: p.dex, str_max: p.str_max, int_max: p.int_max, dex_max: p.dex_max },
            statuses: p.status.clone(),
            ..Default::default()
        }
    }
}

/// What the dock was asked for this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum DockEvent {
    Action(Action),
    /// use what is on the slot
    Slot(usize),
    /// pin something else on the slot: a right click, or Y on the pad
    SlotAssign(usize),
    Button(DockButtonId),
}

/// The pad's moves while its cursor is on the dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavOp {
    Left,
    Right,
    Up,
    Down,
    Select,
    Assign,
    Cancel,
}

/// A value easing to where it is going, as a CSS transition does.
#[derive(Clone, Copy, Debug)]
struct Ease {
    from: f32,
    to: f32,
    start: f64,
    secs: f32,
    curve: (f32, f32, f32, f32),
}

impl Ease {
    fn new(v: f32, secs: f32, curve: (f32, f32, f32, f32)) -> Ease {
        Ease { from: v, to: v, start: 0.0, secs, curve }
    }
    fn set(&mut self, v: f32, now: f64) {
        if (v - self.to).abs() < 1e-6 {
            return;
        }
        self.from = self.at(now);
        self.to = v;
        self.start = now;
    }
    fn at(&self, now: f64) -> f32 {
        let t = (((now - self.start) as f32) / self.secs).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * cubic_bezier(self.curve, t)
    }
    fn moving(&self, now: f64) -> bool {
        ((now - self.start) as f32) < self.secs && (self.to - self.from).abs() > 1e-6
    }
}

/// CSS's `cubic-bezier(x1, y1, x2, y2)` at time `t`.
pub fn cubic_bezier((x1, y1, x2, y2): (f32, f32, f32, f32), t: f32) -> f32 {
    let bez = |a: f32, b: f32, s: f32| 3.0 * a * s * (1.0 - s).powi(2) + 3.0 * b * s * s * (1.0 - s) + s.powi(3);
    // the curve's parameter whose x is t, by bisection
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if bez(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez(y1, y2, (lo + hi) / 2.0)
}

/// One thing the pad's cursor stops on, on one of the dock's three rows.
#[derive(Clone, Copy, Debug, PartialEq)]
enum StopKind {
    Action(usize),
    Slot(usize),
    Button(usize),
}

#[derive(Clone, Copy, Debug)]
struct DockStop {
    kind: StopKind,
    row: u8,
    rect: Rect,
}

pub struct Dock {
    hp: Ease,
    mp: Ease,
    poison: Ease,
    xp: Ease,
    focus: Option<usize>,
    stops: Vec<DockStop>,
    /// the dock's frame and the row over it, last drawn, for what the layout keeps clear
    frame: Rect,
    above_h: f32,
}

impl Default for Dock {
    fn default() -> Self {
        Self::new()
    }
}

/// The geometry the dock is drawn to: the unit, compact or not, the screen.
#[derive(Clone, Copy, Debug)]
pub struct DockFrame {
    pub screen: Rect,
    pub u: f32,
    pub compact: bool,
    pub landscape: bool,
    /// a coarse pointer: no tooltips on hover
    pub touch: bool,
}

impl DockFrame {
    pub fn new(screen: Rect, ui_scale: f32, touch: bool) -> DockFrame {
        let compact = style::compact_window(screen.size());
        DockFrame { screen, u: if compact { ui_scale } else { style::hud_unit(screen.size(), ui_scale) }, compact, landscape: compact && style::landscape_window(screen.size()), touch }
    }
}

struct Painted<'a> {
    painter: Painter,
    boxes: &'a mut BoxCache,
    tiles: &'a mut TileIcons,
}

/// The tooltip the dock wants up this frame: its card and the element it is over.
pub struct DockTip {
    pub tip: Tip,
    pub over: Rect,
}

impl Dock {
    pub fn new() -> Dock {
        let fill = (0.2, 0.8, 0.3, 1.0);
        Dock { hp: Ease::new(1.0, 0.45, fill), mp: Ease::new(1.0, 0.45, fill), poison: Ease::new(1.0, 0.45, fill), xp: Ease::new(0.0, 0.6, (0.0, 0.0, 0.58, 1.0)), focus: None, stops: vec![], frame: Rect::NOTHING, above_h: 0.0 }
    }

    /// Whether the pad's cursor is on the dock.
    pub fn focused(&self) -> bool {
        self.focus.is_some()
    }

    /// The pad's cursor onto the dock, on its first quick slot holding something (or the first
    /// button with none).
    pub fn focus_on(&mut self, view: &DockView) {
        if self.stops.is_empty() {
            return;
        }
        let first = self.stops.iter().position(|s| matches!(s.kind, StopKind::Slot(i) if view.slots.get(i).is_some_and(|x| x.is_some())));
        self.focus = first.or_else(|| self.stops.iter().position(|s| s.row == 2));
    }

    pub fn focus_off(&mut self) {
        self.focus = None;
    }

    /// What the focused stop does on A and Y, for the prompts under the dock.
    pub fn focus_labels(&self, view: &DockView) -> Option<(String, Option<&'static str>)> {
        let s = self.stops.get(self.focus?)?;
        let tip = self.stop_tip(s.kind, view);
        Some((tip.title, matches!(s.kind, StopKind::Slot(_)).then_some("Put something else here")))
    }

    fn stop_tip(&self, kind: StopKind, view: &DockView) -> Tip {
        match kind {
            StopKind::Action(i) => view.actions.get(i).map(|a| Tip::titled(a.label.clone())).unwrap_or_default(),
            StopKind::Slot(i) => slot_tip(view.slots.get(i).and_then(|s| s.as_ref()), view.device == Device::Pad),
            StopKind::Button(i) => button_tip(&BUTTONS[i]),
        }
    }

    /// A pad press while the cursor is on the dock; false when it is not.
    pub fn nav(&mut self, op: NavOp, view: &DockView) -> Option<DockEvent> {
        let at = self.focus?;
        let cur = *self.stops.get(at)?;
        match op {
            NavOp::Cancel => {
                self.focus = None;
                None
            }
            NavOp::Select => Some(activate(cur.kind, view)),
            NavOp::Assign => match cur.kind {
                StopKind::Slot(i) => Some(DockEvent::SlotAssign(i)),
                _ => None,
            },
            NavOp::Left | NavOp::Right => {
                let row: Vec<usize> = self.stops.iter().enumerate().filter(|(_, s)| s.row == cur.row).map(|(i, _)| i).collect();
                let k = row.iter().position(|&i| i == at).unwrap_or(0) as i32;
                let n = row.len() as i32;
                let next = row[((k + if op == NavOp::Right { 1 } else { -1 }).rem_euclid(n)) as usize];
                self.focus = Some(next);
                None
            }
            NavOp::Up | NavOp::Down => {
                let mut rows: Vec<u8> = self.stops.iter().map(|s| s.row).collect();
                rows.sort();
                rows.dedup();
                let ri = rows.iter().position(|&r| r == cur.row).unwrap_or(0) as i32 + if op == NavOp::Down { 1 } else { -1 };
                if ri < 0 || ri >= rows.len() as i32 {
                    return None;
                }
                let x = cur.rect.center().x;
                let best = self.stops.iter().enumerate().filter(|(_, s)| s.row == rows[ri as usize]).min_by(|a, b| (a.1.rect.center().x - x).abs().total_cmp(&(b.1.rect.center().x - x).abs())).map(|(i, _)| i);
                if best.is_some() {
                    self.focus = best;
                }
                None
            }
        }
    }

    /// The dock's height with the row of actions over it, for the layout to keep the message pane
    /// above both (dock.ts `height`).
    pub fn height(&self, ctx: &egui::Context, f: &DockFrame, view: &DockView) -> f32 {
        if f.compact {
            let (frame, above) = compact_heights(f, view);
            return frame + above;
        }
        let g = Geom::new(ctx, f, view);
        g.frame.height() + g.above_h
    }

    /// The rect the dock's frame took when last drawn.
    pub fn frame_rect(&self) -> Rect {
        self.frame
    }

    /// Draw the dock and take its clicks. Returns what was asked, and the tooltip to show.
    pub fn ui(&mut self, ui: &mut egui::Ui, f: &DockFrame, view: &DockView, tiles: &mut TileIcons, boxes: &mut BoxCache, now: f64) -> (Vec<DockEvent>, Option<DockTip>) {
        let share = |n: i64, max: i64| if max > 0 { (n as f32 / max as f32).clamp(0.0, 1.0) } else { 0.0 };
        self.hp.set(share(view.hp, view.hp_max), now);
        self.mp.set(share(view.mp, view.mp_max), now);
        self.poison.set(share(view.poison_survival, view.hp_max), now);
        self.xp.set(view.progress as f32 / 100.0, now);
        let low = view.hp_max > 0 && view.hp as f32 / view.hp_max as f32 <= 0.25;
        if self.hp.moving(now) || self.mp.moving(now) || self.xp.moving(now) || self.poison.moving(now) || low {
            ui.ctx().request_repaint();
        }
        let painter = ui.painter().clone();
        let mut p = Painted { painter, boxes, tiles };
        let mut events = Vec::new();
        let mut tip = None;
        if f.compact {
            self.compact_ui(ui, f, view, &mut p, now, &mut events, &mut tip);
        } else {
            self.full_ui(ui, f, view, &mut p, now, &mut events, &mut tip, low);
        }
        if f.touch {
            tip = None;
        }
        // the pad's cursor shows its stop's card, as a hover does
        if let Some(s) = self.focus.and_then(|i| self.stops.get(i)).copied() {
            tip = Some(DockTip { tip: self.stop_tip(s.kind, view), over: s.rect });
            p.painter.rect_stroke(s.rect.expand(1.0), 4.0 * f.u, egui::Stroke::new(2.0, BRONZE_BRIGHT.to_color32()), egui::StrokeKind::Outside);
        }
        if !events.is_empty() {
            self.focus = None;
        }
        (events, tip)
    }

    #[allow(clippy::too_many_arguments)]
    fn full_ui(&mut self, ui: &mut egui::Ui, f: &DockFrame, view: &DockView, p: &mut Painted, now: f64, events: &mut Vec<DockEvent>, tip: &mut Option<DockTip>, low: bool) {
        let ctx = ui.ctx().clone();
        let g = Geom::new(&ctx, f, view);
        let u = f.u;
        self.frame = g.frame;
        self.above_h = g.above_h;
        let mut stops = Vec::new();
        // a box under the whole frame takes the pointer, so a click on the dock's stone is not a
        // click on the dungeon under it
        ui.interact(g.mid, ui.scope_id().with("dock-frame"), Sense::click());
        for orb in [g.hp_orb, g.mp_orb] {
            ui.interact(orb, ui.scope_id().with(("dock-orb", orb.min.x as i32)), Sense::click());
        }

        // the middle: the stone, the trim along its top and the diamond in the middle of it
        p.boxes.paint_box(&p.painter, "dock-mid", g.mid, &mid_style(u));
        let trim = Rect::from_min_size(egui::pos2(g.mid.left() + g.mid.width() * 0.16, g.mid.top() - 1.0), egui::vec2(g.mid.width() * 0.68, 2.0 * u));
        crate::paint::gradient_rect(&p.painter, trim, false, &[(0.0, TRANSPARENT), (0.25, BRONZE), (0.5, BRONZE_BRIGHT), (0.75, BRONZE), (1.0, TRANSPARENT)]);
        diamond(&p.painter, egui::pos2(g.mid.center().x, g.mid.top() - 6.0 * u + 5.0 * u), 5.0 * u, IRON_DEEP, IRON_DEEP, BRONZE_BRIGHT, None);

        // the quick slots
        if !view.spectating {
            for (i, rect) in g.slots.iter().enumerate() {
                let slot = view.slots.get(i).and_then(|s| s.as_ref());
                let resp = ui.interact(*rect, ui.scope_id().with(("dock-slot", i)), Sense::click());
                paint_slot(p, *rect, slot, resp.hovered(), u, ICON.slot * u, view.device == Device::Pad, false);
                if resp.clicked() {
                    events.push(if slot.is_some() { DockEvent::Slot(i) } else { DockEvent::SlotAssign(i) });
                }
                if resp.secondary_clicked() {
                    events.push(DockEvent::SlotAssign(i));
                }
                if resp.hovered() {
                    *tip = Some(DockTip { tip: slot_tip(slot, view.device == Device::Pad), over: *rect });
                }
                stops.push(DockStop { kind: StopKind::Slot(i), row: 1, rect: *rect });
            }
            if view.device == Device::Pad {
                let gs = glyph_size(Glyph::L3, 22.0 * u);
                let first = g.slots[0];
                let at = egui::pos2(first.left() - 8.0 * u - gs.x, first.center().y - gs.y / 2.0);
                paint_glyph(&p.painter, Glyph::L3, view.pad, at, 22.0 * u, Color32::WHITE);
                let r = Rect::from_min_size(at, gs);
                if ui.interact(r, ui.scope_id().with("slots-pad"), Sense::hover()).hovered() {
                    *tip = Some(DockTip { tip: Tip::titled("The dock on the pad").body("The hotbar, the actions and the screens").hint(KeyHint::default().pad(Glyph::L3)), over: r });
                }
            }
        }

        // the experience bar
        let xp_resp = ui.interact(g.xp, ui.scope_id().with("dock-xp"), Sense::hover());
        paint_xp(p, &g, view, self.xp.at(now), u);
        if xp_resp.hovered() {
            *tip = Some(DockTip { tip: Tip::titled(format!("Level {}", view.xl)).kicker("Experience").body(format!("{}% of the way to level {}", view.progress, view.xl + 1)), over: g.xp });
        }

        // the screen buttons
        if !view.spectating {
            for (i, (b, rect)) in BUTTONS.iter().zip(&g.buttons).enumerate() {
                let resp = ui.interact(*rect, ui.scope_id().with(("dock-button", i)), Sense::click());
                paint_button(p, *rect, b, resp.hovered() || self.focus.and_then(|f| self.stops.get(f)).is_some_and(|s| s.kind == StopKind::Button(i)), u, ICON.button * u, view, 52.0 * u);
                if resp.clicked() {
                    events.push(DockEvent::Button(b.id));
                }
                if resp.hovered() {
                    *tip = Some(DockTip { tip: button_tip(b), over: *rect });
                }
                stops.push(DockStop { kind: StopKind::Button(i), row: 2, rect: *rect });
            }
        }

        // the defences and attributes, the place and the gold
        p.painter.rect_filled(Rect::from_min_size(g.stats.left_top(), egui::vec2(1.0, g.stats.height())), 0.0, hex(0x2c2620).to_color32());
        p.painter.rect_filled(Rect::from_min_size(g.stats.right_top() - egui::vec2(1.0, 0.0), egui::vec2(1.0, g.stats.height())), 0.0, hex(0x2c2620).to_color32());
        for (i, st) in g.stat_cells.iter().enumerate() {
            let resp = ui.interact(st.rect, ui.scope_id().with(("dock-stat", i)), Sense::hover());
            p.painter.galley(st.value_at, st.value.clone(), Color32::WHITE);
            p.painter.galley(st.key_at, st.key.clone(), Color32::WHITE);
            if resp.hovered() {
                *tip = Some(DockTip { tip: st.tip.clone(), over: st.rect });
            }
        }
        let place_resp = ui.interact(g.place, ui.scope_id().with("dock-place"), Sense::hover());
        p.painter.galley(g.place.min, g.place_galley.clone(), Color32::WHITE);
        if place_resp.hovered() {
            *tip = Some(DockTip { tip: Tip::titled(view.place.clone()).body("Where you are"), over: g.place });
        }
        let gold_resp = ui.interact(g.gold, ui.scope_id().with("dock-gold"), Sense::hover());
        let coin = g.gold_galley.size().y.min(14.0 * u) * 0.8;
        coin_at(p, egui::pos2(g.gold.left(), g.gold.center().y - coin / 2.0), coin);
        p.painter.galley(egui::pos2(g.gold.left() + coin + 0.4 * 14.0 * u, g.gold.top()), g.gold_galley.clone(), Color32::WHITE);
        if gold_resp.hovered() {
            *tip = Some(DockTip { tip: Tip::titled(format!("{} gold", view.gold)), over: g.gold });
        }

        // the orbs, over the middle's edges
        for (i, rect) in [g.hp_orb, g.mp_orb].into_iter().enumerate() {
            let hp = i == 0;
            let (fill, poison) = if hp { (self.hp.at(now), (view.poison_survival < view.hp).then(|| self.poison.at(now))) } else { (self.mp.at(now), None) };
            paint_orb(p, rect, hp, fill, poison, if hp { (view.hp, view.hp_max) } else { (view.mp, view.mp_max) }, !hp && view.mp_max <= 0, hp && low, now, u);
            if ui.rect_contains_pointer(rect) {
                let t = if hp {
                    let mut t = Tip::titled("Health").sub(format!("{} of {}", view.hp, view.hp_max));
                    if view.poison_survival < view.hp {
                        t.body = Some(format!("Poison will leave you {}", view.poison_survival));
                    }
                    t
                } else {
                    Tip::titled("Magic").sub(format!("{} of {}", view.mp, view.mp_max))
                };
                *tip = Some(DockTip { tip: t, over: rect });
            }
        }

        // the row over the dock: the status lights at the left, the actions in the middle
        for (s, rect) in &g.chips {
            let resp = ui.interact(*rect, ui.scope_id().with(("dock-chip", rect.min.x as i32, rect.min.y as i32)), Sense::hover());
            paint_chip(p, *rect, s, u, false);
            if resp.hovered() {
                *tip = Some(DockTip { tip: chip_tip(s), over: *rect });
            }
        }
        if !view.spectating {
            for (i, (a, rect)) in view.actions.iter().zip(&g.actions).enumerate() {
                let resp = ui.interact(*rect, ui.scope_id().with(("dock-action", i)), Sense::click());
                paint_action(p, *rect, a, resp.hovered(), u, if is_main(a.id) { ICON.main } else { ICON.action } * u, view.device == Device::Pad, false, false);
                if resp.clicked() {
                    events.push(DockEvent::Action(a.action.clone()));
                }
                if resp.hovered() {
                    *tip = Some(DockTip { tip: Tip::titled(a.label.clone()).hint(a.key.clone()), over: *rect });
                }
                stops.push(DockStop { kind: StopKind::Action(i), row: 0, rect: *rect });
            }
        }
        stops.sort_by_key(|s| s.row);
        self.restops(stops);
    }

    /// The stops, rebuilt each frame; the cursor keeps to the same thing, or to its row.
    fn restops(&mut self, stops: Vec<DockStop>) {
        if let Some(was) = self.focus.and_then(|i| self.stops.get(i)).copied() {
            self.focus = stops.iter().position(|s| s.kind == was.kind).or_else(|| stops.iter().position(|s| s.row == was.row));
        }
        self.stops = stops;
    }

    #[allow(clippy::too_many_arguments)]
    fn compact_ui(&mut self, ui: &mut egui::Ui, f: &DockFrame, view: &DockView, p: &mut Painted, now: f64, events: &mut Vec<DockEvent>, tip: &mut Option<DockTip>) {
        let ctx = ui.ctx().clone();
        let g = CompactGeom::new(&ctx, f, view);
        let u = f.u;
        self.frame = g.frame;
        self.above_h = g.above.height();
        let mut stops = Vec::new();
        ui.interact(g.frame, ui.scope_id().with("dock-frame"), Sense::click());
        // the frame: the stone along the foot with its bronze edge
        p.boxes.paint_box(&p.painter, "dock-compact-frame", g.frame, &BoxStyle {
            layers: vec![Layer::Linear(180.0, vec![sa(hex(0x1d1a17)), s(IRON, 0.4), sa(IRON_DEEP)])],
            border: Border { w: [1.0, 0.0, 0.0, 0.0], c: [BRONZE_DIM; 4] },
            outers: vec![outer(0.0, -8.0, 24.0, 0.0, rgba(0, 0, 0, 0.55))],
            ..Default::default()
        });
        for (i, rect) in [g.hp, g.mp].into_iter().enumerate() {
            let hp = i == 0;
            let fill = if hp { self.hp.at(now) } else { self.mp.at(now) };
            let poison = (hp && view.poison_survival < view.hp).then(|| self.poison.at(now));
            paint_bar_orb(p, rect, hp, fill, poison, if hp { (view.hp, view.hp_max) } else { (view.mp, view.mp_max) }, !hp && view.mp_max <= 0, f.landscape);
        }
        paint_xp_compact(p, g.xp, view, self.xp.at(now), f.landscape);
        if !view.spectating {
            for (i, rect) in g.slots.iter().enumerate() {
                let slot = view.slots.get(i).and_then(|s| s.as_ref());
                let resp = ui.interact(*rect, ui.scope_id().with(("dock-slot", i)), Sense::click());
                paint_slot(p, *rect, slot, resp.hovered(), u, COMPACT_ICON.slot * u, false, true);
                if resp.clicked() {
                    events.push(if slot.is_some() { DockEvent::Slot(i) } else { DockEvent::SlotAssign(i) });
                }
                if resp.secondary_clicked() || resp.long_touched() {
                    events.push(DockEvent::SlotAssign(i));
                }
                stops.push(DockStop { kind: StopKind::Slot(i), row: 1, rect: *rect });
            }
            for (i, (b, rect)) in BUTTONS.iter().zip(&g.buttons).enumerate() {
                let resp = ui.interact(*rect, ui.scope_id().with(("dock-button", i)), Sense::click());
                paint_button(p, *rect, b, resp.hovered(), u, COMPACT_ICON.button * u, &DockView { device: Device::Pointer, ..Default::default() }, rect.width());
                if resp.clicked() {
                    events.push(DockEvent::Button(b.id));
                }
                stops.push(DockStop { kind: StopKind::Button(i), row: 2, rect: *rect });
            }
            for (i, (a, rect)) in view.actions.iter().zip(&g.actions).enumerate() {
                let resp = ui.interact(*rect, ui.scope_id().with(("dock-action", i)), Sense::click());
                let main = is_main(a.id);
                paint_action(p, *rect, a, resp.hovered(), u, if main { COMPACT_ICON.main } else { COMPACT_ICON.action } * u, false, true, !f.landscape);
                if resp.clicked() {
                    events.push(DockEvent::Action(a.action.clone()));
                }
                stops.push(DockStop { kind: StopKind::Action(i), row: 0, rect: *rect });
            }
        }
        for (s, rect) in &g.chips {
            let resp = ui.interact(*rect, ui.scope_id().with(("dock-chip", rect.min.x as i32, rect.min.y as i32)), Sense::hover());
            paint_chip(p, *rect, s, u, true);
            if resp.hovered() {
                *tip = Some(DockTip { tip: chip_tip(s), over: *rect });
            }
        }
        stops.sort_by_key(|s| s.row);
        self.restops(stops);
    }
}

fn activate(kind: StopKind, view: &DockView) -> DockEvent {
    match kind {
        StopKind::Action(i) => DockEvent::Action(view.actions[i].action.clone()),
        StopKind::Slot(i) => {
            if view.slots.get(i).is_some_and(|s| s.is_some()) {
                DockEvent::Slot(i)
            } else {
                DockEvent::SlotAssign(i)
            }
        }
        StopKind::Button(i) => DockEvent::Button(BUTTONS[i].id),
    }
}

fn button_tip(b: &ButtonDef) -> Tip {
    let mut hint = KeyHint::keys(b.keys);
    hint.pad = b.pad;
    Tip::titled(b.label).body(b.about).hint(hint)
}

fn slot_kind(r: &SlotRef) -> &'static str {
    match r {
        SlotRef::Spell(_) => "Spell",
        SlotRef::Ability(_) => "Ability",
        SlotRef::Item(_) => "Item",
        SlotRef::Fire => "Quiver",
    }
}

/// The keys a slot is on (dock.ts `slotHint`).
fn slot_hint(slot: &QuickSlot) -> KeyHint {
    match &slot.r {
        SlotRef::Spell(l) => KeyHint::keys(&["z", l]).label("Cast"),
        SlotRef::Ability(l) => KeyHint::keys(&["a", l]).label("Use"),
        SlotRef::Fire => KeyHint::keys(&["f"]).pad(Glyph::RB).label("Fire"),
        SlotRef::Item(_) => {
            let key = crate::gear::use_key(slot.verb.as_deref());
            let keys: Vec<String> = match (key, &slot.letter) {
                (Some(k), Some(l)) => vec![k.to_string(), l.clone()],
                _ => vec![],
            };
            KeyHint { keys, label: Some(slot.verb.clone().unwrap_or_else(|| "Use".into())), ..Default::default() }
        }
    }
}

fn slot_tip(slot: Option<&QuickSlot>, pad: bool) -> Tip {
    let Some(slot) = slot else {
        let hint = if pad { KeyHint::default().pad(Glyph::A).label("Choose") } else { KeyHint::keys(&["Click"]).label("Choose") };
        return Tip::titled("Empty slot").body("A spell, an ability or an item goes here").hint(hint);
    };
    let hint = slot_hint(slot);
    let mut hints = vec![];
    if !hint.keys.is_empty() || hint.pad.is_some() {
        hints.push(hint);
    }
    hints.push(if pad { KeyHint::default().pad(Glyph::Y).label("Change") } else { KeyHint::keys(&["Right click"]).label("Change") });
    Tip {
        kicker: Some(format!("{}{}", slot_kind(&slot.r), if slot.pinned { " · pinned" } else { "" })),
        title: formatted_string_to_text(&slot.label),
        body: slot.useless.then(|| "Of no use to you".to_string()),
        hints,
        ..Default::default()
    }
}

fn status_colour(s: &StatusEntry) -> C {
    style::term(s.col.unwrap_or(7))
}

fn chip_tip(s: &StatusEntry) -> Tip {
    let light = s.light.clone().unwrap_or_default();
    let about = s.desc.clone().filter(|d| !d.trim().is_empty()).or_else(|| s.text.clone()).unwrap_or_default().trim().to_string();
    let mut t = Tip::titled(light.clone());
    t.colour = Some(status_colour(s).mix_oklab(hex(0xffffff), 0.6));
    if !about.is_empty() && about != light {
        t.body = Some(about);
    }
    t
}

/// A stat of the dock: its number over its key.
struct StatCell {
    rect: Rect,
    value: std::sync::Arc<egui::Galley>,
    value_at: Pos2,
    key: std::sync::Arc<egui::Galley>,
    key_at: Pos2,
    tip: Tip,
}

/// Where everything of the full dock stands this frame.
struct Geom {
    frame: Rect,
    above_h: f32,
    mid: Rect,
    hp_orb: Rect,
    mp_orb: Rect,
    slots: Vec<Rect>,
    xp: Rect,
    buttons: Vec<Rect>,
    stats: Rect,
    stat_cells: Vec<StatCell>,
    place: Rect,
    place_galley: std::sync::Arc<egui::Galley>,
    gold: Rect,
    gold_galley: std::sync::Arc<egui::Galley>,
    actions: Vec<Rect>,
    chips: Vec<(StatusEntry, Rect)>,
}

type Galley = std::sync::Arc<egui::Galley>;

/// The six stats as columns of the grid: each one's left edge, its number, its key and its card;
/// and the grid's width.
fn stat_cells(ctx: &egui::Context, s: &DockStats, u: f32) -> (Vec<(f32, Galley, Galley, Tip)>, f32) {
    let key_st = TextStyle::new(Face::Caps700, 9.5 * u, hex(0x857759)).spacing(0.14).line(1.0);
    let mut out = vec![];
    let defence = |label: &str, name: &str, v: i64, change: i64, about: &str| {
        let colour = if change > 0 {
            BETTER
        } else if change < 0 {
            WORSE
        } else {
            hex(0xefe4cc)
        };
        let vg = TextStyle::new(Face::Caps700, 19.0 * u, colour).line(1.0).galley(ctx, &v.to_string(), None);
        let body = if change != 0 { format!("{about}\n{}{change} for now", if change > 0 { "+" } else { "" }) } else { about.to_string() };
        (vg, key_st.galley(ctx, label, None), Tip::titled(format!("{name} {v}")).body(body))
    };
    let attribute = |label: &str, name: &str, v: i64, max: i64| {
        let drained = v < max;
        let vg = TextStyle::new(Face::Caps700, 16.0 * u, if drained { WORSE } else { hex(0xc9bea6) }).line(1.0).galley(ctx, &v.to_string(), None);
        let mut t = Tip::titled(format!("{name} {v}"));
        if drained {
            t.body = Some(format!("Drained: {v} of {max}"));
        }
        (vg, key_st.galley(ctx, label, None), t)
    };
    let cells = [
        defence("AC", "Armour class", s.ac, s.ac_mod, "How much of a blow your armour takes"),
        defence("EV", "Evasion", s.ev, s.ev_mod, "How often blows and missiles miss you"),
        defence("SH", "Shield", s.sh, s.sh_mod, "How often your shield blocks"),
        attribute("Str", "Strength", s.str, s.str_max),
        attribute("Int", "Intelligence", s.int, s.int_max),
        attribute("Dex", "Dexterity", s.dex, s.dex_max),
    ];
    let mut width = 0.0;
    for (i, (v, k, t)) in cells.into_iter().enumerate() {
        let w = v.size().x.max(k.size().x);
        // the grid: three columns, a 6-unit one, three more, 12 apart
        if i == 3 {
            width += 6.0 * u + 12.0 * u;
        }
        out.push((width, v, k, t));
        width += w + 12.0 * u;
    }
    (out, width - 12.0 * u)
}

impl Geom {
    fn new(ctx: &egui::Context, f: &DockFrame, view: &DockView) -> Geom {
        let u = f.u;
        let orb = 156.0 * u;
        let slot = 60.0 * u;
        let watch = view.spectating;
        let slots_w = SLOT_COUNT as f32 * slot + (SLOT_COUNT - 1) as f32 * 5.0 * u + 14.0 * u;
        let buttons_w = 7.0 * 52.0 * u + 6.0 * 5.0 * u;
        let (cells, cells_w) = stat_cells(ctx, &view.stats, u);
        let stats_w = cells_w + 32.0 * u + 2.0;
        let stat_h = 19.0 * u + 2.0 * u + 9.5 * u;
        let place_galley = TextStyle::new(Face::Caps700, 17.0 * u, hex(0xefe4cc)).spacing(0.06).line(1.0).galley(ctx, &view.place, None);
        let gold_galley = TextStyle::new(Face::Caps700, 14.0 * u, GOLD_BRIGHT).line(1.0).galley(ctx, &view.gold.to_string(), None);
        let coin = 14.0 * u * 0.8;
        let gold_w = coin + 0.4 * 14.0 * u + gold_galley.size().x;
        let info_w = place_galley.size().x.max(gold_w);
        let info_h = 17.0 * u + 4.0 * u + 14.0 * u;
        let row_w = if watch { stats_w + 16.0 * u + info_w } else { buttons_w + 16.0 * u + stats_w + 16.0 * u + info_w };
        let content_w = if watch { row_w } else { row_w.max(slots_w) };
        let padx = orb * 0.2 + 22.0 * u;
        let mid_w = content_w + 2.0 * padx + 2.0;
        let row_h = if watch { stat_h.max(info_h) } else { (52.0 * u).max(stat_h).max(info_h) };
        let mid_h = 1.0 + 14.0 * u + if watch { 0.0 } else { slot + 9.0 * u } + 26.0 * u + 9.0 * u + row_h + 10.0 * u;
        let frame_h = mid_h.max(orb + 6.0 * u);
        let frame_w = orb + mid_w - 0.4 * orb + orb;
        let left = (f.screen.center().x - frame_w / 2.0).round();
        let bottom = f.screen.bottom();
        let frame = Rect::from_min_size(egui::pos2(left, bottom - frame_h), egui::vec2(frame_w, frame_h));
        let hp_orb = Rect::from_min_size(egui::pos2(left, bottom - 6.0 * u - orb), Vec2::splat(orb));
        let mid = Rect::from_min_size(egui::pos2(left + orb * 0.8, bottom - mid_h), egui::vec2(mid_w, mid_h));
        let mp_orb = Rect::from_min_size(egui::pos2(mid.right() - orb * 0.2, bottom - 6.0 * u - orb), Vec2::splat(orb));
        let cl = mid.left() + 1.0 + padx;
        let cr = mid.right() - 1.0 - padx;
        let mut y = mid.top() + 1.0 + 14.0 * u;
        let mut slots = vec![];
        if !watch {
            let mut x = cl;
            for i in 0..SLOT_COUNT {
                slots.push(Rect::from_min_size(egui::pos2(x, y), Vec2::splat(slot)));
                x += slot + 5.0 * u + if i == 4 { 14.0 * u } else { 0.0 };
            }
            y += slot + 9.0 * u;
        }
        let xp = Rect::from_min_max(egui::pos2(cl, y), egui::pos2(cr, y + 26.0 * u));
        y += 26.0 * u + 9.0 * u;
        let row = Rect::from_min_max(egui::pos2(cl, y), egui::pos2(cr, y + row_h));
        let mut x = cl;
        let mut buttons = vec![];
        if !watch {
            for _ in 0..7 {
                buttons.push(Rect::from_min_size(egui::pos2(x, row.center().y - 26.0 * u), Vec2::splat(52.0 * u)));
                x += 52.0 * u + 5.0 * u;
            }
            x += 16.0 * u - 5.0 * u;
        }
        let stats = Rect::from_min_size(egui::pos2(x, row.center().y - stat_h / 2.0), egui::vec2(stats_w, stat_h));
        let mut stat_rects = vec![];
        for (cx, v, k, t) in cells {
            let w = v.size().x.max(k.size().x);
            let col_left = stats.left() + 1.0 + 16.0 * u + cx;
            // aligned at the foot: the key's line at the bottom, the number over it
            let key_top = stats.bottom() - 9.5 * u;
            let val_top = key_top - 2.0 * u - v.size().y;
            let rect = Rect::from_min_max(egui::pos2(col_left, val_top), egui::pos2(col_left + w, stats.bottom()));
            stat_rects.push(StatCell { rect, value_at: egui::pos2(col_left + (w - v.size().x) / 2.0, val_top), key_at: egui::pos2(col_left + (w - k.size().x) / 2.0, key_top), value: v, key: k, tip: t });
        }
        let info_top = row.center().y - info_h / 2.0;
        let place = Rect::from_min_size(egui::pos2(cr - place_galley.size().x, info_top), egui::vec2(place_galley.size().x, 17.0 * u));
        let gold = Rect::from_min_size(egui::pos2(cr - gold_w, info_top + 17.0 * u + 4.0 * u), egui::vec2(gold_w, 14.0 * u));

        // the row over the frame: the actions centred, the status chips from the left
        let actions = if watch { vec![] } else { action_rects(ctx, &view.actions, u, frame) };
        let tallest = actions.iter().map(|r| r.height()).fold(0.0, f32::max);
        let above_h = (72.0 * u).max(12.0 * u + tallest);
        let chips = chip_rects(ctx, &view.statuses, u, egui::pos2(frame.left() + orb * 0.1, frame.top() - 12.0 * u), orb * 2.2, false);
        Geom { frame, above_h, mid, hp_orb, mp_orb, slots, xp, buttons, stats, stat_cells: stat_rects, place, place_galley, gold, gold_galley, actions, chips }
    }
}

fn action_label_style(u: f32, compact: bool) -> TextStyle {
    TextStyle::new(Face::Caps700, if compact { 12.0 } else { 18.0 * u }, BRONZE_BRIGHT).spacing(0.04).line(1.0)
}

/// The label of a lit action, cut at its `max-width` with an ellipsis.
fn action_label(ctx: &egui::Context, label: &str, u: f32, compact: bool) -> std::sync::Arc<egui::Galley> {
    let st = action_label_style(u, compact);
    let max = st.size * if compact { 8.5 } else { 18.0 };
    let g = st.galley(ctx, label, None);
    if g.size().x <= max {
        return g;
    }
    let chars: Vec<char> = label.chars().collect();
    for n in (1..chars.len()).rev() {
        let t: String = chars[..n].iter().collect::<String>() + "…";
        let g = st.galley(ctx, &t, None);
        if g.size().x <= max {
            return g;
        }
    }
    g
}

/// The actions' boxes in a row centred over `frame`, standing on its top 12 units up.
fn action_rects(ctx: &egui::Context, actions: &[DockAction], u: f32, frame: Rect) -> Vec<Rect> {
    let sizes: Vec<Vec2> = actions
        .iter()
        .map(|a| {
            if a.here {
                let label = action_label(ctx, &a.label, u, false);
                egui::vec2(8.0 * u + ICON.action * u + 8.0 * u + label.size().x + 18.0 * u + 2.0, 64.0 * u)
            } else {
                egui::vec2(58.0 * u, 58.0 * u)
            }
        })
        .collect();
    let total: f32 = sizes.iter().map(|s| s.x).sum::<f32>() + 14.0 * u * sizes.len().saturating_sub(1) as f32;
    let mut x = frame.center().x - total / 2.0;
    let foot = frame.top() - 12.0 * u;
    sizes
        .into_iter()
        .map(|s| {
            let r = Rect::from_min_size(egui::pos2(x, foot - s.y), s);
            x += s.x + 14.0 * u;
            r
        })
        .collect()
}

fn chip_galley(ctx: &egui::Context, s: &StatusEntry, u: f32, compact: bool) -> std::sync::Arc<egui::Galley> {
    let ink = status_colour(s).mix_oklab(hex(0xffffff), 0.6);
    TextStyle::new(Face::Caps700, if compact { 10.5 } else { 13.0 * u }, ink).spacing(0.05).line(1.0).galley(ctx, s.light.as_deref().unwrap_or(""), None)
}

/// The status lights' chips: from `foot_left` (their bottom-left corner) rightward, wrapping
/// upward past `max_w` (`flex-wrap: wrap-reverse`); compact, a column down from the top left.
fn chip_rects(ctx: &egui::Context, statuses: &[StatusEntry], u: f32, foot_left: Pos2, max_w: f32, compact: bool) -> Vec<(StatusEntry, Rect)> {
    let lights: Vec<&StatusEntry> = statuses.iter().filter(|s| s.light.as_ref().is_some_and(|l| !l.is_empty())).collect();
    let (h, pad_l, pad_r, gem, gap_in, gap) = if compact { (22.0, 7.0, 7.0, 8.0 * u, 5.0, 4.0) } else { (28.0 * u, 9.0 * u, 11.0 * u, 8.0 * u, 7.0 * u, 5.0 * u) };
    let widths: Vec<f32> = lights.iter().map(|s| pad_l + gem + gap_in + chip_galley(ctx, s, u, compact).size().x + pad_r + 2.0).collect();
    let mut out = vec![];
    if compact {
        let mut y = foot_left.y;
        for (s, w) in lights.iter().zip(widths) {
            out.push(((*s).clone(), Rect::from_min_size(egui::pos2(foot_left.x, y), egui::vec2(w, h))));
            y += h + gap;
        }
        return out;
    }
    // lines, filled left to right, the first at the bottom
    let mut lines: Vec<Vec<(usize, f32)>> = vec![vec![]];
    let mut x = 0.0;
    for (i, w) in widths.iter().enumerate() {
        if x > 0.0 && x + w > max_w {
            lines.push(vec![]);
            x = 0.0;
        }
        lines.last_mut().unwrap().push((i, *w));
        x += w + gap;
    }
    for (li, line) in lines.iter().enumerate() {
        let top = foot_left.y - h - li as f32 * (h + gap);
        let mut x = foot_left.x;
        for &(i, w) in line {
            out.push((lights[i].clone(), Rect::from_min_size(egui::pos2(x, top), egui::vec2(w, h))));
            x += w + gap;
        }
    }
    out
}

/// `.dock-mid`
fn mid_style(u: f32) -> BoxStyle {
    BoxStyle {
        layers: vec![
            Layer::RepeatingLinear(90.0, vec![Stop(rgba(255, 255, 255, 0.012), At::Pt(0.0)), Stop(rgba(255, 255, 255, 0.012), At::Pt(2.0 * u)), Stop(TRANSPARENT, At::Pt(2.0 * u)), Stop(TRANSPARENT, At::Pt(7.0 * u))]),
            Layer::Linear(180.0, vec![sa(hex(0x1d1a17)), s(IRON, 0.4), sa(IRON_DEEP)]),
        ],
        radius: [8.0 * u, 8.0 * u, 0.0, 0.0],
        border: Border { w: [1.0, 1.0, 0.0, 1.0], c: [BRONZE_DIM, hex(0x3a3128), hex(0x3a3128), hex(0x3a3128)] },
        insets: vec![inset(0.0, 1.0, 0.0, 0.0, rgba(255, 236, 200, 0.08)), inset(0.0, 0.0, 0.0, 4.0 * u, rgba(0, 0, 0, 0.3))],
        outers: vec![outer(0.0, -2.0, 0.0, 0.0, rgba(0, 0, 0, 0.6)), outer(0.0, -12.0 * u, 34.0 * u, 0.0, rgba(0, 0, 0, 0.6))],
        ..Default::default()
    }
}

/// A square turned 45 degrees about `centre`, `half` its half-diagonal: a gradient from its top
/// corner to its bottom one, a border, and an outer ring.
fn diamond(painter: &Painter, centre: Pos2, half: f32, top: C, bottom: C, border: C, ring: Option<C>) {
    if let Some(r) = ring {
        let h = half + 1.4;
        painter.add(egui::Shape::convex_polygon(vec![centre + egui::vec2(0.0, -h), centre + egui::vec2(h, 0.0), centre + egui::vec2(0.0, h), centre + egui::vec2(-h, 0.0)], r.to_color32(), egui::Stroke::NONE));
    }
    let mut mesh = egui::Mesh::default();
    let mid = top.mix(bottom, 0.5).to_color32();
    mesh.colored_vertex(centre + egui::vec2(0.0, -half), top.to_color32());
    mesh.colored_vertex(centre + egui::vec2(half, 0.0), mid);
    mesh.colored_vertex(centre + egui::vec2(0.0, half), bottom.to_color32());
    mesh.colored_vertex(centre + egui::vec2(-half, 0.0), mid);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
    let pts = vec![centre + egui::vec2(0.0, -half), centre + egui::vec2(half, 0.0), centre + egui::vec2(0.0, half), centre + egui::vec2(-half, 0.0)];
    painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.0, border.to_color32())));
}

/// `.coin`
fn coin_at(p: &mut Painted, at: Pos2, size: f32) {
    let r = Rect::from_min_size(at, Vec2::splat(size));
    p.painter.circle_filled(r.center(), size / 2.0 + 1.0, hex(0x3b2804).to_color32());
    p.boxes.paint_box(&p.painter, "coin", r, &BoxStyle { layers: vec![Layer::Radial(RadialSize::CircleFarthest, (0.35, 0.35), vec![sa(hex(0xfff2b0)), s(hex(0xe0aa2c), 0.55), sa(hex(0x7b5208))])], ..Default::default() }.round(size / 2.0));
}

/// A quick slot (`.slot`).
#[allow(clippy::too_many_arguments)]
fn paint_slot(p: &mut Painted, rect: Rect, slot: Option<&QuickSlot>, hover: bool, u: f32, icon: f32, pad: bool, compact: bool) {
    let kind = slot.map(|s| s.r.kind()).unwrap_or("");
    let border = if hover {
        BRONZE_BRIGHT
    } else if slot.is_some_and(|s| s.pinned) {
        BRONZE_DIM
    } else if slot.is_some() {
        hex(0x4a3f31)
    } else {
        hex(0x3a3128)
    };
    let band = match kind {
        "spell" => Some(rgba(160, 110, 230, 0.55)),
        "ability" => Some(rgba(230, 180, 80, 0.55)),
        "fire" => Some(rgba(230, 100, 60, 0.55)),
        _ => None,
    };
    let mut insets = vec![inset(0.0, 0.0, 0.0, 1.0, hex(0x000000)), inset(0.0, 3.0 * u, 10.0 * u, 0.0, rgba(0, 0, 0, 0.9))];
    if let Some(b) = band {
        insets.push(inset(0.0, -3.0 * u, 0.0, 0.0, b));
    }
    let key = format!("slot:{kind}:{:?}", border);
    let st = BoxStyle {
        layers: vec![Layer::Radial(RadialSize::CircleFarthest, (0.5, 0.35), vec![sa(hex(0x1b1815)), s(hex(0x070606), 0.8)])],
        radius: [4.0 * u; 4],
        border: Border::all(1.0, border),
        insets,
        outers: if band.is_none() { vec![outer(0.0, 1.0, 0.0, 0.0, rgba(255, 236, 200, 0.05))] } else { vec![] },
        ..Default::default()
    };
    p.boxes.paint_box(&p.painter, &key, rect, &st);
    let Some(slot) = slot else {
        if hover {
            p.painter.text(rect.center(), egui::Align2::CENTER_CENTER, "+", style::font(Face::Mono, if compact { 24.0 } else { 24.0 * u }), BRONZE.to_color32());
        }
        return;
    };
    let cell = Rect::from_center_size(rect.center(), Vec2::splat(icon.min(rect.width())));
    let placed: Vec<Placed> = if slot.r == SlotRef::Fire && slot.tiles.is_empty() { icons::icon_tiles(p.tiles, &DockIcon::Gui("CMD_AUTOFIGHT")) } else { icons::layer_tiles(p.tiles, &slot.tiles) };
    // `.slot.useless canvas`: grey and darker
    let tint = if slot.useless { Color32::from_gray(140) } else { Color32::WHITE };
    icons::paint_with_shadow(p.tiles, &p.painter, &placed, cell, tint, 2.0 * u, 2.0 * u, Color32::BLACK);
    if let Some(n) = slot.count.filter(|n| *n > 1) {
        let size = if compact { 11.0 } else { 14.0 * u };
        let g = TextStyle::new(Face::MonoBold, size, hex(0xffffff)).line(1.0).galley(p.painter.ctx(), &n.to_string(), None);
        let (r, b) = if compact { (2.0, 0.0) } else { (4.0 * u, 2.0 * u) };
        let at = egui::pos2(rect.right() - 1.0 - r - g.size().x, rect.bottom() - 1.0 - b - g.size().y);
        crate::paint::shadowed_galley(&p.painter, at, g, Color32::WHITE, &[(0.0, 0.0, 2.0, Color32::BLACK), (0.0, 1.0, 1.0, Color32::BLACK)]);
    }
    if !compact && !pad {
        let hint = slot_hint(slot);
        if !hint.keys.is_empty() {
            let size = KeycapSize::slot(u);
            let at = egui::pos2(rect.left() - 3.0 * u, rect.bottom() + 6.0 * u - size.h);
            paint_keycaps(&p.painter, p.boxes, at, &hint.keys, size, u);
        }
    }
}

/// A screen button (`.dock-button`), round.
#[allow(clippy::too_many_arguments)]
fn paint_button(p: &mut Painted, rect: Rect, b: &ButtonDef, hover: bool, u: f32, icon: f32, view: &DockView, size: f32) {
    let border = if hover { BRONZE_BRIGHT } else { hex(0x3a3128) };
    let st = BoxStyle {
        layers: vec![Layer::Radial(RadialSize::CircleFarthest, (0.5, 0.3), vec![sa(hex(0x25211c)), s(hex(0x0d0c0a), 0.8)])],
        radius: [size / 2.0; 4],
        border: Border::all(1.0, border),
        insets: vec![inset(0.0, 1.0, 0.0, 0.0, rgba(255, 236, 200, 0.07)), inset(0.0, 0.0, 0.0, 3.0 * u, rgba(0, 0, 0, 0.35))],
        outers: vec![outer(0.0, 2.0 * u, 4.0 * u, 0.0, rgba(0, 0, 0, 0.6))],
        ..Default::default()
    };
    p.boxes.paint_box(&p.painter, &format!("dock-button:{hover}"), rect, &st);
    let cell = Rect::from_center_size(rect.center(), Vec2::splat(icon));
    let placed = icons::icon_tiles(p.tiles, &DockIcon::Gui(b.icon));
    let tint = Color32::WHITE.gamma_multiply(if hover { 1.0 } else { 0.86 });
    icons::paint_with_shadow(p.tiles, &p.painter, &placed, cell, tint, 1.0, 1.0, Color32::BLACK);
    if view.device == Device::Pad {
        if let Some(g) = b.pad {
            let gs = glyph_size(g, 18.0 * u);
            paint_glyph(&p.painter, g, view.pad, egui::pos2(rect.right() + 6.0 * u - gs.x, rect.bottom() + 6.0 * u - gs.y), 18.0 * u, Color32::WHITE);
        }
    }
}

/// An action over the dock (`.dock-action`): lit (`.here`) with its label, or a square.
#[allow(clippy::too_many_arguments)]
fn paint_action(p: &mut Painted, rect: Rect, a: &DockAction, hover: bool, u: f32, icon: f32, pad: bool, compact: bool, portrait: bool) {
    let main = is_main(a.id);
    let lit = a.here;
    let border = if hover {
        BRONZE_BRIGHT
    } else if lit || (compact && main) {
        BRONZE
    } else {
        hex(0x3f352a)
    };
    let mut outers = vec![outer(0.0, 3.0 * u, 10.0 * u, 0.0, rgba(0, 0, 0, 0.6))];
    if compact && main {
        outers = vec![outer(0.0, 0.0, 12.0, 0.0, rgba(232, 201, 138, 0.4))];
    } else if compact && portrait && lit {
        outers = vec![outer(0.0, 0.0, 10.0, 0.0, rgba(232, 201, 138, 0.35))];
    }
    let layers = if lit {
        vec![Layer::Radial(RadialSize::EllipseFarthest, (0.3, 0.3), vec![sa(rgba(58, 46, 28, 0.96)), s(rgba(14, 12, 9, 0.96), 0.8)])]
    } else {
        vec![Layer::Radial(RadialSize::CircleFarthest, (0.5, 0.3), vec![sa(rgba(40, 36, 31, 0.95)), s(rgba(10, 9, 8, 0.95), 0.8)])]
    };
    let st = BoxStyle { layers, radius: [5.0 * u; 4], border: Border::all(1.0, border), insets: vec![inset(0.0, 0.0, 0.0, 1.0, hex(0x000000)), inset(0.0, 1.0, 0.0, 0.0, rgba(255, 236, 200, 0.07))], outers, ..Default::default() };
    p.boxes.paint_box(&p.painter, &format!("dock-action:{lit}:{:?}:{main}:{compact}", border), rect, &st);
    let label = (lit && !(compact && portrait)).then(|| action_label(p.painter.ctx(), &a.label, u, compact));
    let content_w = icon + label.as_ref().map_or(0.0, |l| (if compact { 4.0 } else { 8.0 * u }) + l.size().x);
    let left = if label.is_some() { rect.left() + 1.0 + if compact { 4.0 } else { 8.0 * u } } else { rect.center().x - content_w / 2.0 };
    let cell = Rect::from_min_size(egui::pos2(left, rect.center().y - icon / 2.0), Vec2::splat(icon));
    let placed = icons::icon_tiles(p.tiles, &a.icon);
    icons::paint_with_shadow(p.tiles, &p.painter, &placed, cell, Color32::WHITE, 2.0 * u, 2.0 * u, Color32::BLACK);
    if let Some(l) = label {
        let at = egui::pos2(cell.right() + if compact { 4.0 } else { 8.0 * u }, rect.center().y - l.size().y / 2.0);
        crate::paint::shadowed_galley(&p.painter, at, l, BRONZE_BRIGHT.to_color32(), &[(0.0, 1.0, 2.0, Color32::BLACK)]);
    }
    if !compact && !pad && !a.key.keys.is_empty() {
        let size = KeycapSize::action(u);
        let w = keycaps_width(p.painter.ctx(), &a.key.keys, size, u);
        paint_keycaps(&p.painter, p.boxes, egui::pos2(rect.right() + 4.0 * u - w, rect.bottom() + 7.0 * u - size.h), &a.key.keys, size, u);
    }
}

/// A status light's chip (`.status-chip`).
fn paint_chip(p: &mut Painted, rect: Rect, s: &StatusEntry, u: f32, compact: bool) {
    let c = status_colour(s);
    let border = c.mix(IRON_LINE, 0.35);
    let st = BoxStyle {
        layers: vec![Layer::Linear(180.0, vec![sa(rgba(26, 24, 22, 0.94)), sa(rgba(10, 9, 8, 0.94))])],
        radius: [3.0 * u; 4],
        border: Border::all(1.0, border),
        outers: vec![outer(0.0, 2.0, 6.0, 0.0, rgba(0, 0, 0, 0.55))],
        ..Default::default()
    };
    p.boxes.paint_box(&p.painter, &format!("chip:{}", s.col.unwrap_or(7)), rect, &st);
    let (pad_l, gem, gap) = if compact { (7.0, 8.0 * u, 5.0) } else { (9.0 * u, 8.0 * u, 7.0 * u) };
    let gc = egui::pos2(rect.left() + 1.0 + pad_l + gem / 2.0, rect.center().y);
    // the gem: a turned square in the light's colour, a dark ring and a glow of it
    p.painter.circle_filled(gc, gem * 0.9, c.alpha(0.25).to_color32());
    diamond(&p.painter, gc, gem * 0.7071 + 1.0, rgba(0, 0, 0, 0.8), rgba(0, 0, 0, 0.8), rgba(0, 0, 0, 0.8), None);
    diamond(&p.painter, gc, gem * 0.7071, c, c, c, None);
    let g = chip_galley(p.painter.ctx(), s, u, compact);
    let at = egui::pos2(gc.x + gem / 2.0 + gap, rect.center().y - g.size().y / 2.0);
    let ink = c.mix_oklab(hex(0xffffff), 0.6).to_color32();
    crate::paint::shadowed_galley(&p.painter, at, g, ink, &[(0.0, 1.0, 1.0, Color32::BLACK)]);
}

/// The experience row (`.dock-xp`): the level in its diamond, the track with its tenths.
fn paint_xp(p: &mut Painted, g: &Geom, view: &DockView, fill: f32, u: f32) {
    let lv = Rect::from_min_size(egui::pos2(g.xp.left(), g.xp.center().y - 13.0 * u), Vec2::splat(26.0 * u));
    // `.xp-level::before`: a square inset 3 units, turned, so its half-diagonal is 10 units * √2
    diamond(&p.painter, lv.center(), 10.0 * u * std::f32::consts::SQRT_2, hex(0x2b241b), hex(0x0b0908), BRONZE, Some(hex(0x000000)));
    let t = TextStyle::new(Face::Caps700, 13.0 * u, BRONZE_BRIGHT).line(1.0).galley(p.painter.ctx(), &view.xl.to_string(), None);
    p.painter.galley(lv.center() - t.size() / 2.0, t, Color32::WHITE);
    let track = Rect::from_min_max(egui::pos2(lv.right() + 10.0 * u, g.xp.center().y - 4.0 * u), egui::pos2(g.xp.right(), g.xp.center().y + 4.0 * u));
    paint_track(p, track, fill, u, false);
}

fn paint_track(p: &mut Painted, track: Rect, fill: f32, u: f32, compact: bool) {
    let r = if compact { 4.0 } else { 4.0 * u };
    p.boxes.paint_box(&p.painter, "xp-track", track, &BoxStyle {
        layers: vec![Layer::Solid(IRON_DEEP)],
        radius: [r; 4],
        border: Border::all(1.0, hex(0x3a3128)),
        insets: vec![inset(0.0, 1.0, 2.0, 0.0, hex(0x000000))],
        ..Default::default()
    });
    let inner = track.shrink(1.0);
    let w = inner.width() * fill.clamp(0.0, 1.0);
    if w > 0.0 {
        let fr = Rect::from_min_size(inner.min, egui::vec2(w, inner.height()));
        crate::paint::gradient_rect(&p.painter, fr, true, &[(0.0, hex(0xf1d58e)), (0.6, hex(0xc08d34)), (1.0, hex(0x8a611c))]);
    }
    // the tenths: a dark line at the end of every tenth of the track
    for i in 1..=10 {
        let x = inner.left() + inner.width() * i as f32 / 10.0;
        p.painter.rect_filled(Rect::from_min_max(egui::pos2(x - 1.0, inner.top()), egui::pos2(x, inner.bottom())), 0.0, rgba(0, 0, 0, 0.75).to_color32());
    }
}

/// `.orb`: the stone ring, the well, the liquid at its level, the glass, the numbers.
#[allow(clippy::too_many_arguments)]
fn paint_orb(p: &mut Painted, rect: Rect, hp: bool, fill: f32, poison: Option<f32>, (now_v, of): (i64, i64), none: bool, low: bool, now: f64, u: f32) {
    let size = rect.width();
    // `.orb::before`: the stone
    crate::paint::outer_shadow(&p.painter, rect, [size / 2.0; 4], outer(0.0, 6.0 * u, 18.0 * u, 0.0, rgba(0, 0, 0, 0.75)));
    p.painter.circle_filled(rect.center(), size / 2.0 + 1.0, Color32::BLACK);
    p.boxes.paint_box(&p.painter, "orb-stone", rect, &BoxStyle {
        layers: vec![
            Layer::Radial(RadialSize::CircleFarthest, (0.5, 0.5), vec![s(TRANSPARENT, 0.6), s(rgba(0, 0, 0, 0.5), 0.61), s(TRANSPARENT, 0.66)]),
            Layer::Conic(200.0, (0.5, 0.5), vec![sa(hex(0x3a3329)), s(hex(0x121110), 0.25), s(hex(0x2c2620), 0.5), s(hex(0x0f0e0d), 0.75), sa(hex(0x3a3329))]),
        ],
        radius: [size / 2.0; 4],
        border: Border::all(1.0, BRONZE_DIM),
        insets: vec![inset(0.0, 0.0, 0.0, 3.0 * u, rgba(0, 0, 0, 0.35))],
        ..Default::default()
    });
    // `.orb-well`: inset 10%, its rings outside it
    let well = rect.shrink(size * 0.1);
    let wr = well.width() / 2.0;
    p.painter.circle_filled(well.center(), wr + 4.0 * u, Color32::BLACK);
    p.painter.circle_filled(well.center(), wr + 3.0 * u, BRONZE.to_color32());
    p.painter.circle_filled(well.center(), wr + 1.0, Color32::BLACK);
    p.boxes.paint_box(&p.painter, "orb-well", well, &BoxStyle { layers: vec![Layer::Radial(RadialSize::CircleFarthest, (0.5, 0.4), vec![sa(hex(0x151313)), s(hex(0x050404), 0.75)])], ..Default::default() }.round(wr));
    // the liquid: a well-sized box moved down by what is missing, seen through the well
    let liquid = liquid_texture(p, well.size(), hp, u);
    let level = well.top() + well.height() * (1.0 - fill);
    let tex_rect = well.translate(egui::vec2(0.0, well.height() * (1.0 - fill)));
    crate::paint::textured_polygon(&p.painter, &circle_below(well.center(), wr, level), liquid, tex_rect, Color32::WHITE);
    if let Some(pf) = poison {
        // `.orb-poison`: a green wash from the level poison will leave, under a dashed line
        let py = well.top() + well.height() * (1.0 - pf);
        let band = p.boxes.texture(p.painter.ctx(), "orb-poison", well.size(), || BoxStyle { layers: vec![Layer::Linear(180.0, vec![sa(rgba(110, 200, 40, 0.18)), s(TRANSPARENT, 0.3)])], ..Default::default() });
        crate::paint::textured_polygon(&p.painter, &circle_below(well.center(), wr, py), band.id(), well.translate(egui::vec2(0.0, py - well.top())), Color32::WHITE);
        let dy = py - well.center().y;
        if dy.abs() < wr {
            let half = (wr * wr - dy * dy).sqrt();
            let mut x = well.center().x - half;
            while x < well.center().x + half {
                let e = (x + 6.0).min(well.center().x + half);
                p.painter.line_segment([egui::pos2(x, py + 1.0), egui::pos2(e, py + 1.0)], egui::Stroke::new(2.0, hex(0x8ae234).to_color32()));
                x += 12.0;
            }
        }
    }
    // `.orb-glass`
    p.boxes.paint_box(&p.painter, "orb-glass", well, &BoxStyle {
        layers: vec![
            Layer::Radial(RadialSize::Ellipse(0.3, 0.13), (0.4, 0.15), vec![sa(rgba(255, 255, 255, 0.26)), s(TRANSPARENT, 0.75)]),
            Layer::Radial(RadialSize::Ellipse(0.5, 0.14), (0.5, 0.92), vec![sa(rgba(255, 255, 255, 0.1)), s(TRANSPARENT, 0.7)]),
            Layer::Radial(RadialSize::CircleFarthest, (0.5, 0.45), vec![s(TRANSPARENT, 0.55), s(rgba(0, 0, 0, 0.6), 1.0)]),
        ],
        ..Default::default()
    }.round(wr));
    // `.orb-rim`: the low health pulse, a red glow round the well
    if low {
        let t = (now % 1.8) as f32 / 0.9;
        let k = if t <= 1.0 { t } else { 2.0 - t };
        let o = cubic_bezier((0.42, 0.0, 0.58, 1.0), k);
        let glow = egui::epaint::RectShape::filled(well.expand(3.0 * u), egui::CornerRadius::same((wr + 3.0 * u).min(255.0) as u8), rgba(255, 50, 30, 0.85 * o).to_color32()).with_blur_width(16.0 * u);
        p.painter.add(glow);
        p.painter.circle_filled(well.center(), wr - 1.0, Color32::TRANSPARENT);
    }
    // `.orb-text`: 700 20 units of Cinzel, the slash and the maximum fainter
    let ctx = p.painter.ctx().clone();
    let alpha = if none { 0.35 } else { 1.0 };
    let ink = hex(0xfbf4e6);
    let mut job = egui::text::LayoutJob::default();
    let fmt = |size: f32, a: f32| egui::TextFormat { font_id: style::font(Face::Caps700, size), color: ink.alpha(a * alpha).to_color32(), extra_letter_spacing: 0.02 * size, valign: egui::Align::BOTTOM, ..Default::default() };
    job.append(&now_v.to_string(), 0.0, fmt(20.0 * u, 1.0));
    job.append("/", 0.12 * 20.0 * u, fmt(20.0 * u, 0.55));
    job.append(&of.to_string(), 0.12 * 20.0 * u, fmt(16.0 * u, 0.75));
    let g = ctx.fonts_mut(|f| f.layout_job(job));
    let at = rect.center() - g.size() / 2.0;
    for (dx, dy, a) in [(0.0, 1.0, 0.9), (1.5, 0.0, 0.35), (-1.5, 0.0, 0.35), (0.0, -1.5, 0.35), (0.0, 2.5, 0.35)] {
        p.painter.galley_with_override_text_color(at + egui::vec2(dx, dy), g.clone(), Color32::BLACK.gamma_multiply(a * alpha));
    }
    p.painter.galley(at, g, Color32::WHITE);
}

/// The liquid's own texture: its colour (`.orb.hp .orb-liquid`, `.orb.mp`) and the light along
/// its top (`::before`).
fn liquid_texture(p: &mut Painted, size: Vec2, hp: bool, u: f32) -> egui::TextureId {
    let key = if hp { "orb-liquid-hp" } else { "orb-liquid-mp" };
    let t = p.boxes.texture(p.painter.ctx(), key, size, || {
        let (glow, top, mid, bottom, shine) = if hp {
            (rgba(255, 120, 90, 0.35), hex(0xc8222c), hex(0x8c0f17), hex(0x4b060b), (255, 190, 170, 0.55))
        } else {
            (rgba(120, 170, 255, 0.35), hex(0x2f62d8), hex(0x173a99), hex(0x0a1a52), (190, 215, 255, 0.6))
        };
        let band = 10.0 * u;
        BoxStyle {
            layers: vec![
                // the shine: a band 10 units deep at the top, bright for two points then fading
                Layer::Linear(180.0, vec![sp(rgba(shine.0, shine.1, shine.2, shine.3), 0.0), sp(rgba(shine.0, shine.1, shine.2, 0.12), 2.0), sp(rgba(shine.0, shine.1, shine.2, 0.0), band), sp(TRANSPARENT, band)]),
                Layer::Radial(RadialSize::CircleFarthest, (0.5, 1.2), vec![sa(glow), s(TRANSPARENT, 0.55)]),
                Layer::Linear(180.0, vec![sa(top), s(mid, 0.55), sa(bottom)]),
            ],
            ..Default::default()
        }
    });
    t.id()
}

/// The part of a circle below `level`, as a convex polygon.
fn circle_below(c: Pos2, r: f32, level: f32) -> Vec<Pos2> {
    let dy = (level - c.y).clamp(-r, r);
    if dy >= r {
        return vec![];
    }
    let a0 = (dy / r).asin();
    let (start, end) = (a0, std::f32::consts::PI - a0);
    let n = 48;
    (0..=n).map(|i| start + (end - start) * i as f32 / n as f32).map(|a| c + egui::vec2(a.cos(), a.sin()) * r).collect()
}

// ---------------------------------------------------------------------------- compact

/// The compact dock's heights: the frame along the foot and the row of actions over it.
fn compact_heights(f: &DockFrame, view: &DockView) -> (f32, f32) {
    let u = f.u;
    let gap = if f.landscape { 3.0 } else { 5.0 };
    let (pt, pb) = if f.landscape { (5.0, 4.0) } else { (7.0, 6.0) };
    let frame_h = if f.landscape {
        let bars: f32 = 15.0 + gap + 15.0 + gap + 16.0;
        let slots = if view.spectating { 0.0 } else { 34.0 };
        let buttons = if view.spectating { 0.0 } else { 42.0 };
        pt + bars.max(slots).max(buttons) + pb + 1.0
    } else {
        let slot = ((f.screen.width() - 16.0 - 9.0 * 3.0) / 10.0).min(34.0);
        let mut h = 18.0 + gap + 20.0;
        if !view.spectating {
            h += gap + slot + gap + 46.0;
        }
        pt + h + pb + 1.0
    };
    let tallest: f32 = if view.spectating || view.actions.is_empty() {
        0.0
    } else {
        view.actions.iter().map(|a| if is_main(a.id) { 58.0 } else if f.landscape { 44.0 } else { 42.0 }).fold(0.0, f32::max)
    };
    let _ = u;
    (frame_h, 50.0f32.max(tallest + 6.0))
}

struct CompactGeom {
    frame: Rect,
    above: Rect,
    hp: Rect,
    mp: Rect,
    xp: Rect,
    slots: Vec<Rect>,
    buttons: Vec<Rect>,
    actions: Vec<Rect>,
    chips: Vec<(StatusEntry, Rect)>,
}

impl CompactGeom {
    fn new(ctx: &egui::Context, f: &DockFrame, view: &DockView) -> CompactGeom {
        let u = f.u;
        let (frame_h, above_h) = compact_heights(f, view);
        let sc = f.screen;
        let frame = Rect::from_min_max(egui::pos2(sc.left(), sc.bottom() - frame_h), sc.max);
        let above = Rect::from_min_max(egui::pos2(sc.left(), frame.top() - above_h), egui::pos2(sc.right(), frame.top()));
        let (pl, pr) = (8.0, 8.0);
        let inner = Rect::from_min_max(egui::pos2(frame.left() + pl, frame.top() + 1.0 + if f.landscape { 5.0 } else { 7.0 }), egui::pos2(frame.right() - pr, frame.bottom() - if f.landscape { 4.0 } else { 6.0 }));
        let (hp, mp, xp, slots, buttons);
        if f.landscape {
            // three columns: the bars, the slots, the buttons
            let gap_x = 10.0;
            let buttons_w = if view.spectating { 0.0 } else { 7.0 * 42.0 + 6.0 * 4.0 };
            let col1 = (inner.width() * 0.2).clamp(110.0, 160.0);
            let rows_h = 15.0 + 3.0 + 15.0 + 3.0 + 16.0;
            let top = inner.center().y - rows_h / 2.0;
            hp = Rect::from_min_size(egui::pos2(inner.left(), top), egui::vec2(col1, 15.0));
            mp = Rect::from_min_size(egui::pos2(inner.left(), top + 18.0), egui::vec2(col1, 15.0));
            xp = Rect::from_min_size(egui::pos2(inner.left(), top + 36.0), egui::vec2(col1, 16.0));
            let slots_left = inner.left() + col1 + gap_x;
            let slots_right = inner.right() - buttons_w - if buttons_w > 0.0 { gap_x } else { 0.0 };
            let mut s = vec![];
            if !view.spectating {
                let mut x = slots_left;
                for _ in 0..SLOT_COUNT {
                    if x + 34.0 > slots_right + 0.5 {
                        break;
                    }
                    s.push(Rect::from_min_size(egui::pos2(x, inner.center().y - 17.0), Vec2::splat(34.0)));
                    x += 34.0 + 3.0;
                }
            }
            slots = s;
            let mut b = vec![];
            if !view.spectating {
                let mut x = inner.right() - buttons_w;
                for _ in 0..7 {
                    b.push(Rect::from_min_size(egui::pos2(x, inner.center().y - 21.0), Vec2::splat(42.0)));
                    x += 42.0 + 4.0;
                }
            }
            buttons = b;
        } else {
            let col = (inner.width() - 8.0) / 2.0;
            hp = Rect::from_min_size(inner.min, egui::vec2(col, 18.0));
            mp = Rect::from_min_size(egui::pos2(inner.left() + col + 8.0, inner.top()), egui::vec2(col, 18.0));
            xp = Rect::from_min_size(egui::pos2(inner.left(), inner.top() + 18.0 + 5.0), egui::vec2(inner.width(), 20.0));
            let mut y = xp.bottom() + 5.0;
            let mut s = vec![];
            let mut b = vec![];
            if !view.spectating {
                let slot = ((inner.width() - 9.0 * 3.0) / 10.0).min(34.0);
                let total = 10.0 * slot + 9.0 * 3.0;
                let mut x = inner.center().x - total / 2.0;
                for _ in 0..SLOT_COUNT {
                    s.push(Rect::from_min_size(egui::pos2(x, y), Vec2::splat(slot)));
                    x += slot + 3.0;
                }
                y += slot + 5.0;
                // the buttons spread across (`justify-content: space-between`)
                let step = (inner.width() - 46.0) / 6.0;
                for i in 0..7 {
                    b.push(Rect::from_min_size(egui::pos2(inner.left() + step * i as f32, y), Vec2::splat(46.0)));
                }
            }
            slots = s;
            buttons = b;
        }
        // the actions: a row at the right end of the strip over the dock, the main one last
        let mut actions = vec![Rect::NOTHING; view.actions.len()];
        if !view.spectating {
            let gap = if f.landscape { 6.0 } else { 5.0 };
            let foot = above.bottom() - 6.0;
            let mut order: Vec<usize> = (0..view.actions.len()).filter(|&i| !is_main(view.actions[i].id)).collect();
            order.extend((0..view.actions.len()).filter(|&i| is_main(view.actions[i].id)));
            let mut x = above.right() - 8.0;
            for &i in order.iter().rev() {
                let a = &view.actions[i];
                let (w, h) = if is_main(a.id) {
                    (58.0, 58.0)
                } else if a.here && f.landscape {
                    let l = action_label(ctx, &a.label, u, true);
                    (4.0 + COMPACT_ICON.action * u + 4.0 + l.size().x + 10.0 + 2.0, 44.0)
                } else if f.landscape {
                    (44.0, 44.0)
                } else {
                    (42.0, 42.0)
                };
                x -= w;
                actions[i] = Rect::from_min_size(egui::pos2(x, foot - h), egui::vec2(w, h));
                x -= gap;
            }
        }
        // the status lights stand in a column at the top left, under the corner's buttons
        let chips = chip_rects(ctx, &view.statuses, u, egui::pos2(sc.left() + 8.0, sc.top() + 8.0 + 50.0), sc.width() * 0.45, true);
        CompactGeom { frame, above, hp, mp, xp, slots, buttons, actions, chips }
    }
}

/// The compact dock's bar in place of an orb.
#[allow(clippy::too_many_arguments)]
fn paint_bar_orb(p: &mut Painted, rect: Rect, hp: bool, fill: f32, poison: Option<f32>, (now_v, of): (i64, i64), none: bool, landscape: bool) {
    // `.orb-well`: rounded 3, a black and a bronze ring outside
    p.painter.rect_filled(rect.expand(2.0), 4.0, BRONZE_DIM.to_color32());
    p.painter.rect_filled(rect.expand(1.0), 3.5, Color32::BLACK);
    p.boxes.paint_box(&p.painter, "bar-well", rect, &BoxStyle { layers: vec![Layer::Radial(RadialSize::CircleFarthest, (0.5, 0.4), vec![sa(hex(0x151313)), s(hex(0x050404), 0.75)])], ..Default::default() }.round(3.0));
    let w = rect.width() * fill.clamp(0.0, 1.0);
    if w > 0.0 {
        let (top, mid, bottom) = if hp { (hex(0xe0404a), hex(0xa3151e), hex(0x5e0a10)) } else { (hex(0x4a7ff0), hex(0x1d48b5), hex(0x0e2466)) };
        let fr = Rect::from_min_size(rect.min, egui::vec2(w, rect.height()));
        let key = if hp { "bar-hp" } else { "bar-mp" };
        // the liquid's texture is the whole bar moved left by what is missing, rounded at its ends
        let t = p.boxes.texture(p.painter.ctx(), key, rect.size(), || BoxStyle { layers: vec![Layer::Linear(180.0, vec![sa(top), s(mid, 0.55), sa(bottom)])], ..Default::default() }.round(3.0));
        let uv = Rect::from_min_max(egui::pos2(1.0 - fill, 0.0), egui::pos2(1.0, 1.0));
        p.painter.image(t.id(), fr, uv, Color32::WHITE);
    }
    if let Some(pf) = poison {
        let x = rect.left() + rect.width() * pf;
        let mut y = rect.top();
        while y < rect.bottom() {
            p.painter.line_segment([egui::pos2(x, y), egui::pos2(x, (y + 3.0).min(rect.bottom()))], egui::Stroke::new(2.0, hex(0x8ae234).to_color32()));
            y += 6.0;
        }
    }
    // `.orb-glass`, flat: light at the top, dark at the foot
    p.boxes.paint_box(&p.painter, "bar-glass", rect, &BoxStyle { layers: vec![Layer::Linear(180.0, vec![sa(rgba(255, 255, 255, 0.2)), s(TRANSPARENT, 0.45), sa(rgba(0, 0, 0, 0.3))])], ..Default::default() }.round(3.0));
    let size = if landscape { 11.0 } else { 12.0 };
    let alpha = if none { 0.35 } else { 1.0 };
    let ink = hex(0xfbf4e6);
    let mut job = egui::text::LayoutJob::default();
    let fmt = |sz: f32, a: f32| egui::TextFormat { font_id: style::font(Face::Caps700, sz), color: ink.alpha(a * alpha).to_color32(), extra_letter_spacing: 0.02 * sz, valign: egui::Align::BOTTOM, ..Default::default() };
    job.append(&now_v.to_string(), 0.0, fmt(size, 1.0));
    job.append("/", 0.12 * size, fmt(size, 0.55));
    job.append(&of.to_string(), 0.12 * size, fmt(size * 0.8, 0.75));
    let g = p.painter.ctx().fonts_mut(|f| f.layout_job(job));
    let at = rect.center() - g.size() / 2.0;
    p.painter.galley_with_override_text_color(at + egui::vec2(0.0, 1.0), g.clone(), Color32::BLACK.gamma_multiply(0.9 * alpha));
    p.painter.galley(at, g, Color32::WHITE);
}

fn paint_xp_compact(p: &mut Painted, rect: Rect, view: &DockView, fill: f32, landscape: bool) {
    let side = if landscape { 16.0 } else { 20.0 };
    let lv = Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - side / 2.0), Vec2::splat(side));
    // the turned square stands 3 points in from the level's box
    diamond(&p.painter, lv.center(), (side / 2.0 - 3.0) * std::f32::consts::SQRT_2, hex(0x2b241b), hex(0x0b0908), BRONZE, Some(hex(0x000000)));
    let t = TextStyle::new(Face::Caps700, if landscape { 10.0 } else { 11.0 }, BRONZE_BRIGHT).line(1.0).galley(p.painter.ctx(), &view.xl.to_string(), None);
    p.painter.galley(lv.center() - t.size() / 2.0, t, Color32::WHITE);
    let track = Rect::from_min_max(egui::pos2(lv.right() + 6.0, rect.center().y - 2.5), egui::pos2(rect.right(), rect.center().y + 2.5));
    paint_track(p, track, fill, 1.0, true);
}

/// Where the dock's tooltip goes: centred over what it is about, kept on the screen, 10 points
/// over it (dock.ts `placeTip`).
pub fn place_tip(screen: Rect, over: Rect, size: Vec2) -> Pos2 {
    let left = (over.center().x - size.x / 2.0).min(screen.right() - size.x - 4.0).max(screen.left() + 4.0);
    egui::pos2(left, over.top() - size.y - 10.0)
}

/// Lay out and paint a dock tooltip on the tooltip layer.
pub fn paint_tip(ctx: &egui::Context, boxes: &mut BoxCache, screen: Rect, t: &DockTip, u: f32, pad: Option<PadKind>) {
    let card = layout_card(ctx, &t.tip, CardKind::Plain, u, pad);
    let at = place_tip(screen, t.over, card.size);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("hud-dock-tip")));
    card.paint(&painter, boxes, at, pad.unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_ends_where_it_goes() {
        let c = (0.2, 0.8, 0.3, 1.0);
        assert!(cubic_bezier(c, 0.0).abs() < 1e-4);
        assert!((cubic_bezier(c, 1.0) - 1.0).abs() < 1e-4);
        assert!(cubic_bezier(c, 0.3) > 0.5);
    }

    #[test]
    fn tips_keep_to_the_screen() {
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 800.0));
        let at = place_tip(screen, Rect::from_min_size(egui::pos2(990.0, 700.0), egui::vec2(10.0, 10.0)), egui::vec2(200.0, 100.0));
        assert_eq!(at, egui::pos2(796.0, 590.0));
    }

    #[test]
    fn slot_tips() {
        let t = slot_tip(None, false);
        assert_eq!(t.title, "Empty slot");
        let s = QuickSlot { r: SlotRef::Spell("a".into()), label: "Magic Dart".into(), tiles: vec![], count: None, useless: false, pinned: true, verb: None, letter: None };
        let t = slot_tip(Some(&s), false);
        assert_eq!(t.kicker.as_deref(), Some("Spell · pinned"));
        assert_eq!(t.hints[0].keys, ["z", "a"]);
        assert_eq!(t.hints[1].keys, ["Right click"]);
    }
}
