//! The Hints setting, for the pad (the port of `gamepad-hints.ts`): `Adaptive` shows contextual
//! prompts and teaches pad controls until each is used, nothing standing. `Contextual` skips
//! the teaching; `Off` hides gameplay prompts (menu and targeting controls remain). The
//! keyboard gets no prompts.

use crate::bindings::{bar_labels, binding_table, prompt_labels, threatened, Action, BindingLabel, MenuOp, UiOp};
use crate::context::{Context, Mode};
use crate::focus::FocusOp;
use crate::gamepad::Button;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HintMode {
    Adaptive,
    Contextual,
    Off,
}

impl HintMode {
    /// The setting's value (servers.ts `hints`): an unknown one is Adaptive.
    pub fn parse(s: &str) -> Self {
        match s {
            "contextual" => HintMode::Contextual,
            "off" => HintMode::Off,
            _ => HintMode::Adaptive,
        }
    }
}

pub const STORAGE_KEY: &str = "isorun.gamepad-learning.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadLesson {
    Move,
    Look,
    Commands,
    Equipment,
    Explore,
    Examine,
    Travel,
    Wait,
    Rest,
    Navigate,
    TargetCursor,
    MapCursor,
    Fight,
}

pub const LESSONS: [PadLesson; 13] = [
    PadLesson::Move,
    PadLesson::Look,
    PadLesson::Commands,
    PadLesson::Equipment,
    PadLesson::Explore,
    PadLesson::Examine,
    PadLesson::Travel,
    PadLesson::Wait,
    PadLesson::Rest,
    PadLesson::Navigate,
    PadLesson::TargetCursor,
    PadLesson::MapCursor,
    PadLesson::Fight,
];

impl PadLesson {
    pub fn id(self) -> &'static str {
        match self {
            PadLesson::Move => "move",
            PadLesson::Look => "look",
            PadLesson::Commands => "commands",
            PadLesson::Equipment => "equipment",
            PadLesson::Explore => "explore",
            PadLesson::Examine => "examine",
            PadLesson::Travel => "travel",
            PadLesson::Wait => "wait",
            PadLesson::Rest => "rest",
            PadLesson::Navigate => "navigate",
            PadLesson::TargetCursor => "target-cursor",
            PadLesson::MapCursor => "map-cursor",
            PadLesson::Fight => "fight",
        }
    }
}

const STEPS_TO_LEARN: u32 = 3;
/// radians of actual camera movement, not stick polling ticks
const LOOK_TO_LEARN: f32 = 0.3;
const RESPONSE_WINDOW: f64 = 1500.0;

fn key_lesson(text: &str) -> Option<PadLesson> {
    match text {
        "o" => Some(PadLesson::Explore),
        "." => Some(PadLesson::Wait),
        "5" => Some(PadLesson::Rest),
        _ => None,
    }
}

/// Only observable outcomes count; sending a key is not proof that it worked.
#[derive(Clone, Debug, PartialEq)]
pub struct PadHintEvidence {
    pub mode: Mode,
    pub turn: i64,
    pub x: i32,
    pub y: i32,
    pub cursor: String,
    pub focus: i64,
    pub client_overlay: bool,
}

impl Default for PadHintEvidence {
    fn default() -> Self {
        PadHintEvidence { mode: Mode::Command, turn: 1, x: 10, y: 10, cursor: String::new(), focus: -1, client_overlay: false }
    }
}

/// Stable action identities, deliberately independent of button, direction and controller family.
pub fn pad_lesson(a: &Action, ctx: &Context) -> Option<PadLesson> {
    if ctx.mode == Mode::Command {
        match a {
            Action::Fight => return Some(PadLesson::Fight),
            Action::Ui { op: UiOp::Commands, .. } => return Some(PadLesson::Commands),
            Action::Ui { op: UiOp::Equipment, .. } => return Some(PadLesson::Equipment),
            // Select's level map is the way across the dungeon
            Action::Ui { op: UiOp::Levelmap, .. } => return Some(PadLesson::Travel),
            Action::Examine => return Some(PadLesson::Examine),
            Action::Keys { seq, .. } if seq.len() == 1 => {
                if let crate::bindings::Stroke::Text(t) = &seq[0].stroke {
                    return key_lesson(t);
                }
            }
            _ => {}
        }
    }
    match a {
        Action::Menu { op: MenuOp::Next | MenuOp::Prev | MenuOp::Left | MenuOp::Right }
        | Action::Focus { op: FocusOp::Next | FocusOp::Prev | FocusOp::Left | FocusOp::Right } => Some(PadLesson::Navigate),
        Action::Cursor { .. } => match ctx.mode {
            Mode::Targeting => Some(PadLesson::TargetCursor),
            Mode::Levelmap => Some(PadLesson::MapCursor),
            _ => None,
        },
        _ => None,
    }
}

/// Where the lessons learned are kept: the device's storage (`isorun_client::platform::storage`).
pub trait HintStore {
    fn get(&self) -> Option<String>;
    /// May fail (blocked storage): the learning then lasts the session.
    fn set(&mut self, value: &str);
}

/// The device's own storage, under STORAGE_KEY.
pub struct DeviceStore;

impl HintStore for DeviceStore {
    fn get(&self) -> Option<String> {
        isorun_client::platform::storage::get_raw(STORAGE_KEY)
    }
    fn set(&mut self, value: &str) {
        isorun_client::platform::storage::set_raw(STORAGE_KEY, Some(value));
    }
}

/// Device-wide learning, never attached to a character, server or run.
pub struct GamepadHints {
    learned: Vec<PadLesson>,
    steps: u32,
    looked: f32,
    pending: Option<(PadLesson, PadHintEvidence, f64)>,
    store: Box<dyn HintStore>,
}

impl GamepadHints {
    /// The hints over the device's storage.
    pub fn new() -> Self {
        GamepadHints::with_store(Box::new(DeviceStore))
    }

    pub fn with_store(store: Box<dyn HintStore>) -> Self {
        let mut learned = Vec::new();
        // unavailable or old/corrupt storage: teach normally
        if let Some(saved) = store.get().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) {
            if let Some(list) = saved.as_array() {
                for id in LESSONS {
                    if list.iter().any(|v| v.as_str() == Some(id.id())) {
                        learned.push(id);
                    }
                }
            }
        }
        GamepadHints { learned, steps: 0, looked: 0.0, pending: None, store }
    }

    pub fn knows(&self, id: PadLesson) -> bool {
        self.learned.contains(&id)
    }

    fn learn(&mut self, id: PadLesson) {
        if self.knows(id) {
            return;
        }
        self.learned.push(id);
        self.save();
    }

    fn save(&mut self) {
        let list: Vec<&str> = self.learned.iter().map(|l| l.id()).collect();
        self.store.set(&serde_json::to_string(&list).unwrap_or_default());
    }

    pub fn reset(&mut self) {
        self.learned.clear();
        self.steps = 0;
        self.looked = 0.0;
        self.cancel();
        self.save();
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// A lesson's outcome is still awaited (`observe` has something to compare), so the frame loop keeps looking.
    pub fn waiting(&self) -> bool {
        self.pending.is_some()
    }

    /// Called only for server-confirmed, player-directed pad steps, never bumps or autoexplore.
    pub fn moved(&mut self, steps: u32) {
        self.steps += steps;
        if self.steps >= STEPS_TO_LEARN {
            self.learn(PadLesson::Move);
        }
    }

    /// Called while the right stick actually steers the game camera.
    pub fn looked_by(&mut self, radians: f32) {
        self.looked += radians.abs();
        if self.looked >= LOOK_TO_LEARN {
            self.learn(PadLesson::Look);
        }
    }

    pub fn attempt(&mut self, a: &Action, ctx: &Context, before: PadHintEvidence, now: f64) {
        let Some(lesson) = pad_lesson(a, ctx).filter(|l| !self.knows(*l)) else {
            self.cancel();
            return;
        };
        // A held navigation direction must not replace its baseline before the reply arrives.
        if let Some((l, _, at)) = &self.pending {
            if *l == lesson && now - at < RESPONSE_WINDOW {
                return;
            }
        }
        self.pending = Some((lesson, before, now));
    }

    pub fn observe(&mut self, after: &PadHintEvidence, now: f64) {
        let Some((lesson, b, at)) = &self.pending else { return };
        if now - at > RESPONSE_WINDOW {
            self.cancel();
            return;
        }
        let worked = match lesson {
            // X opens one of crawl's menus, or the frame an empty tab stands in
            PadLesson::Commands => (after.mode == Mode::Menu && b.mode != Mode::Menu) || (!b.client_overlay && after.client_overlay),
            PadLesson::Travel => after.mode == Mode::Levelmap && b.mode != Mode::Levelmap,
            // Y opens crawl's own pack
            PadLesson::Equipment => after.mode == Mode::Menu && b.mode != Mode::Menu,
            PadLesson::Examine => after.mode == Mode::Targeting && b.mode != Mode::Targeting,
            PadLesson::Explore => after.x != b.x || after.y != b.y,
            // autofight either swings or takes a step toward the threat; both spend the turn
            PadLesson::Wait | PadLesson::Rest | PadLesson::Fight => after.turn > b.turn,
            PadLesson::Navigate => after.mode == b.mode && after.focus != b.focus,
            PadLesson::TargetCursor | PadLesson::MapCursor => after.mode == b.mode && after.cursor != b.cursor,
            PadLesson::Move | PadLesson::Look => false,
        };
        if worked {
            let l = *lesson;
            self.learn(l);
            self.cancel();
        }
    }

    /// Context first, then the lessons: at most two basic ones, then one discovery. Nothing stands once learned.
    pub fn prompts(&self, ctx: &Context, mode: HintMode) -> Vec<BindingLabel> {
        if matches!(ctx.mode, Mode::Spectating | Mode::Lobby | Mode::Ended | Mode::Macro | Mode::Text) {
            return Vec::new();
        }
        let all = bar_labels(ctx);
        // Confirmation is navigation, not a tutorial reminder. B is always implicit.
        let navigation: Vec<BindingLabel> =
            if matches!(ctx.mode, Mode::Menu | Mode::Targeting | Mode::Levelmap | Mode::Popup | Mode::Newgame | Mode::Crt | Mode::Dialog) {
                all.iter().filter(|l| l.button == Button::A || (ctx.mode == Mode::Menu && l.button == Button::Start)).cloned().collect()
            } else {
                Vec::new()
            };
        let decision = matches!(ctx.mode, Mode::More | Mode::Prompt | Mode::Yesno);
        let mut contextual = if mode == HintMode::Off && !decision { navigation.clone() } else { prompt_labels(ctx) };
        for l in &navigation {
            if !contextual.iter().any(|p| p.button == l.button) {
                contextual.push(l.clone());
            }
        }
        if mode != HintMode::Adaptive {
            return contextual;
        }
        let tip =
            |button: Button, label: &str, action: Action| BindingLabel { button, label: label.into(), action, contextual: false, teaching: true, hold: None };
        let mut teaching: Vec<BindingLabel> = Vec::new();
        if ctx.mode == Mode::Command && threatened(ctx) {
            // The one thing worth teaching mid-fight, and only until it has been used once.
            if !self.knows(PadLesson::Fight) {
                if let Some(l) = all.iter().find(|l| l.button == Button::RT) {
                    teaching.push(BindingLabel { teaching: true, ..l.clone() });
                }
            }
        } else if ctx.mode == Mode::Command && ctx.hostiles_in_view == 0 {
            if !self.knows(PadLesson::Move) {
                teaching.push(tip(Button::LStick, "Move", Action::step(0)));
            }
            if !self.knows(PadLesson::Look) {
                teaching.push(tip(Button::RStick, "Look around", Action::Look { dx: 0.0, dy: 0.0 }));
            }
            if teaching.is_empty() {
                // the level map (Select) and the other menu buttons stand along the view's foot, so they need no lesson
                let table = binding_table(ctx);
                for button in [Button::R3, Button::LB] {
                    let Some(a) = table.get(button) else { continue };
                    if matches!(a, Action::Hold { .. }) {
                        if self.knows(PadLesson::Wait) && (self.knows(PadLesson::Rest) || !ctx.injured) {
                            continue;
                        }
                    } else {
                        match pad_lesson(a, ctx) {
                            Some(id) if !self.knows(id) => {}
                            _ => continue,
                        }
                    }
                    if let Some(l) = all.iter().find(|l| l.button == button) {
                        teaching.push(BindingLabel { teaching: true, ..l.clone() });
                    }
                    break;
                }
            }
        } else if (ctx.mode == Mode::Menu || ctx.focus.as_ref().is_some_and(|f| f.count > 0)) && !self.knows(PadLesson::Navigate) {
            teaching.push(tip(Button::DPad, "Navigate", Action::focus(FocusOp::Next)));
        } else if ctx.mode == Mode::Targeting && !self.knows(PadLesson::TargetCursor) {
            teaching.push(tip(Button::LStick, "Move cursor", Action::Cursor { dir: 0 }));
        } else if ctx.mode == Mode::Levelmap && !self.knows(PadLesson::MapCursor) {
            teaching.push(tip(Button::LStick, "Move cursor", Action::Cursor { dir: 0 }));
        }
        teaching.retain(|l| !contextual.iter().any(|p| p.button == l.button));
        // Contextual interactions keep the anchor at the corner's foot; lessons stack above them.
        contextual.extend(teaching);
        contextual
    }
}

impl Default for GamepadHints {
    fn default() -> Self {
        GamepadHints::new()
    }
}
