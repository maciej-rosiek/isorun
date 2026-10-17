//! The same things as each crate names them: the input crate's buttons, modes and actions, the
//! HUD's glyphs and actions, the overlays' focus. Each crate was ported apart (so a half-done
//! part never broke another's build), and the game screen is where they meet.

use isorun_hud::{Glyph, KeyStep};
use isorun_input::bindings::{BindingLabel, SeqItem};
use isorun_input::{Action, Button, Mode};

/// A pad button as the HUD draws its glyph.
pub fn glyph(b: Button) -> Glyph {
    match b {
        Button::A => Glyph::A,
        Button::B => Glyph::B,
        Button::X => Glyph::X,
        Button::Y => Glyph::Y,
        Button::LB => Glyph::LB,
        Button::RB => Glyph::RB,
        Button::LT => Glyph::LT,
        Button::RT => Glyph::RT,
        Button::Select => Glyph::Select,
        Button::Start => Glyph::Start,
        Button::L3 => Glyph::L3,
        Button::R3 => Glyph::R3,
        Button::DU => Glyph::DU,
        Button::DD => Glyph::DD,
        Button::DL => Glyph::DL,
        Button::DR => Glyph::DR,
        Button::Home => Glyph::Home,
        Button::LStickUp => Glyph::LStickUp,
        Button::LStick => Glyph::LStick,
        Button::RStick => Glyph::RStick,
        Button::DPad => Glyph::DPad,
    }
}

pub fn pad_kind(k: isorun_input::gamepad::PadKind) -> isorun_hud::PadKind {
    use isorun_input::gamepad::PadKind as P;
    match k {
        P::Xbox => isorun_hud::PadKind::Xbox,
        P::Playstation => isorun_hud::PadKind::PlayStation,
        P::Nintendo => isorun_hud::PadKind::Nintendo,
        P::Generic => isorun_hud::PadKind::Generic,
    }
}

/// The context's mode as the HUD reads it.
pub fn hud_mode(m: Mode) -> isorun_hud::Mode {
    match m {
        Mode::Command => isorun_hud::Mode::Command,
        Mode::Menu => isorun_hud::Mode::Menu,
        Mode::Popup => isorun_hud::Mode::Popup,
        Mode::Levelmap => isorun_hud::Mode::LevelMap,
        Mode::Targeting => isorun_hud::Mode::Targeting,
        Mode::Prompt | Mode::Yesno => isorun_hud::Mode::Prompt,
        _ => isorun_hud::Mode::Other,
    }
}

/// Who spoke last, as the HUD and the overlays name it.
pub fn device(d: isorun_input::router::InputDevice) -> (isorun_hud::Device, isorun_overlays::Device) {
    use isorun_input::router::InputDevice as D;
    match d {
        D::Pad => (isorun_hud::Device::Pad, isorun_overlays::Device::Pad),
        D::Keyboard => (isorun_hud::Device::Keyboard, isorun_overlays::Device::Keyboard),
        D::Pointer => (isorun_hud::Device::Pointer, isorun_overlays::Device::Pointer),
    }
}

/// The pad's prompts as the HUD's corner draws them.
pub fn prompts(labels: &[BindingLabel]) -> Vec<isorun_hud::classic::PadPrompt> {
    labels.iter().map(|l| isorun_hud::classic::PadPrompt { button: glyph(l.button), label: l.label.clone(), hold: l.hold.clone(), teaching: l.teaching }).collect()
}

/// A HUD action as the runner takes it.
pub enum HudRun {
    Action(Action),
    Send(serde_json::Value),
    SendThen(serde_json::Value, Vec<SeqItem>),
}

pub fn seq(steps: &[KeyStep]) -> Vec<SeqItem> {
    steps
        .iter()
        .map(|k| match k {
            KeyStep::Text { text, await_prompt } => SeqItem { stroke: isorun_input::Stroke::Text(text.clone()), await_prompt: *await_prompt },
            KeyStep::Key { key, await_prompt } => SeqItem { stroke: isorun_input::Stroke::Key(*key), await_prompt: *await_prompt },
        })
        .collect()
}

pub fn hud_action(a: isorun_hud::Action) -> HudRun {
    match a {
        isorun_hud::Action::Keys { label, seq: s } => HudRun::Action(Action::keys(seq(&s), &label)),
        isorun_hud::Action::Fire => HudRun::Action(Action::Fire),
        isorun_hud::Action::Send(m) => HudRun::Send(m),
        isorun_hud::Action::SendThen(m, s) => HudRun::SendThen(m, seq(&s)),
    }
}

pub fn focus_op(op: isorun_input::focus::FocusOp) -> isorun_overlays::FocusOp {
    use isorun_input::focus::FocusOp as F;
    use isorun_overlays::FocusOp as O;
    match op {
        F::Next => O::Next,
        F::Prev => O::Prev,
        F::Left => O::Left,
        F::Right => O::Right,
        F::Select => O::Select,
        F::AltSelect => O::AltSelect,
        F::Cancel => O::Cancel,
        F::PageNext => O::PageNext,
        F::PagePrev => O::PagePrev,
        F::First => O::First,
        F::Last => O::Last,
    }
}

/// A menu op of the pad's as the overlays take it (one to one).
pub fn menu_op(op: isorun_input::bindings::MenuOp) -> isorun_overlays::MenuOp {
    use isorun_input::bindings::MenuOp as M;
    use isorun_overlays::MenuOp as O;
    match op {
        M::Next => O::Next,
        M::Prev => O::Prev,
        M::PageNext => O::PageNext,
        M::PagePrev => O::PagePrev,
        M::SectionNext => O::SectionNext,
        M::SectionPrev => O::SectionPrev,
        M::First => O::First,
        M::Last => O::Last,
        M::Select => O::Select,
        M::AltSelect => O::AltSelect,
        M::Examine => O::Examine,
        M::Toggle => O::Toggle,
        M::Cancel => O::Cancel,
        M::Left => O::Left,
        M::Right => O::Right,
    }
}

pub fn focus_info(f: isorun_overlays::FocusInfo) -> isorun_input::focus::FocusInfo {
    isorun_input::focus::FocusInfo { label: f.label, colour: f.colour.map(|c| c as i64), cancel_label: f.cancel_label, alt_label: f.alt_label, index: f.index as i64, count: f.count as i64 }
}
