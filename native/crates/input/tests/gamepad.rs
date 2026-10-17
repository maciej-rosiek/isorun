//! The ports of `gamepad-look.test.ts`, `gamepad-repeat.test.ts`, `pad-keys.test.ts` and
//! `gamepad-hints.test.ts` (less its settings-panel half, which is the settings').

use isorun_input::bindings::{binding_table, Action, SeqItem, UiOp};
use isorun_input::context::{Context, Mode, Target};
use isorun_input::focus::{FocusInfo, FocusOp};
use isorun_input::gamepad::*;
use isorun_input::gamepad_hints::*;
use isorun_input::keys::{KeyInput, KeyLike};
use std::cell::RefCell;
use std::rc::Rc;

fn fake_pad(axes: Vec<f32>) -> RawPad {
    RawPad::standard("Xbox Wireless Controller (045e)", 0, axes)
}

fn poll(gp: &mut GamepadInput, now: f64, pads: Vec<Option<RawPad>>) -> Vec<PadEvent> {
    gp.poll(now, &pads);
    gp.take_events()
}

fn looks(events: &[PadEvent]) -> Vec<(f32, f32)> {
    events.iter().filter_map(|e| if let PadEvent::Look { dx, dy, .. } = e { Some((*dx, *dy)) } else { None }).collect()
}

// ---------------------------------------------------------------------------
// right stick look has hysteresis

#[test]
fn a_stick_resting_off_centre_inside_the_enter_threshold_emits_no_look() {
    let mut gp = GamepadInput::default();
    let mut all = Vec::new();
    for t in 0..10 {
        all.extend(poll(&mut gp, t as f64 * 16.0, vec![Some(fake_pad(vec![0.0, 0.0, 0.2, 0.05]))]));
    }
    assert!(looks(&all).is_empty());
    assert!(!gp.is_looking());
}

#[test]
fn a_deliberate_deflection_starts_a_look_keeps_it_to_the_deadzone_and_settles_once() {
    let mut gp = GamepadInput::default();
    let mut l = looks(&poll(&mut gp, 0.0, vec![Some(fake_pad(vec![0.0, 0.0, 0.6, 0.0]))]));
    assert!(gp.is_looking());
    assert_eq!(l.len(), 1);
    assert!(l[0].0 > 0.0);
    l.extend(looks(&poll(&mut gp, 16.0, vec![Some(fake_pad(vec![0.0, 0.0, 0.2, 0.0]))])));
    assert!(gp.is_looking());
    assert_eq!(l.len(), 2);
    assert!(l[1].0 > 0.0);
    l.extend(looks(&poll(&mut gp, 32.0, vec![Some(fake_pad(vec![0.0, 0.0, 0.1, 0.0]))])));
    l.extend(looks(&poll(&mut gp, 48.0, vec![Some(fake_pad(vec![0.0, 0.0, 0.1, 0.0]))])));
    assert!(!gp.is_looking());
    assert_eq!(l.len(), 3);
    assert_eq!(l[2], (0.0, 0.0));
}

#[test]
fn only_the_first_frame_of_a_look_is_the_pad_speaking() {
    let mut gp = GamepadInput::default();
    let mut events = Vec::new();
    for t in [0.0, 16.0, 32.0] {
        events.extend(poll(&mut gp, t, vec![Some(fake_pad(vec![0.0, 0.0, 0.6, 0.0]))]));
    }
    events.extend(poll(&mut gp, 48.0, vec![Some(fake_pad(vec![0.0, 0.0, 0.1, 0.0]))]));
    assert_eq!(events.iter().map(is_pad_activity).collect::<Vec<_>>(), vec![true, false, false, false]);
    assert_eq!(events.iter().map(|e| matches!(e, PadEvent::Look { start: true, .. })).collect::<Vec<_>>(), vec![true, false, false, false]);
}

// ---------------------------------------------------------------------------
// which pad is primary on a Steam Deck with an external controller

fn raw_pad(index: usize) -> RawPad {
    RawPad { id: "Microsoft X-Box 360 pad (045e-028e)".into(), index, standard: false, buttons: vec![RawButton::default(); 11], axes: vec![0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0] }
}

#[test]
fn a_raw_pad_resting_its_triggers_at_minus_one_does_not_take_the_primary() {
    let mut deck = RawPad { id: "Steam Virtual Gamepad".into(), ..fake_pad(vec![0.0; 4]) };
    let raw = raw_pad(1);
    let mut gp = GamepadInput::default();
    assert!(poll(&mut gp, 0.0, vec![Some(deck.clone()), Some(raw.clone())]).is_empty());
    deck.press(0, true);
    poll(&mut gp, 16.0, vec![Some(deck), Some(raw)]);
    assert!(gp.is_held(Button::A));
}

#[test]
fn the_unmapped_pad_alone_is_read_once_it_is_used() {
    let mut raw = raw_pad(0);
    let mut gp = GamepadInput::default();
    poll(&mut gp, 0.0, vec![Some(raw.clone())]);
    raw.press(0, true);
    poll(&mut gp, 16.0, vec![Some(raw)]);
    assert!(gp.is_held(Button::A));
}

#[test]
fn when_a_steam_input_pad_and_its_raw_twin_wake_together_the_standard_one_is_read() {
    let deck = fake_pad(vec![0.0; 4]);
    let mut virt = RawPad { index: 1, ..fake_pad(vec![0.0; 4]) };
    let mut raw = raw_pad(2);
    let mut gp = GamepadInput::default();
    poll(&mut gp, 0.0, vec![Some(deck.clone()), Some(virt.clone()), Some(raw.clone())]);
    virt.press(8, true);
    raw.press(6, true);
    poll(&mut gp, 16.0, vec![Some(deck), Some(virt), Some(raw)]);
    assert!(gp.is_held(Button::Select));
    assert!(!gp.is_held(Button::LT));
}

// ---------------------------------------------------------------------------
// an unmapped pad is read in the standard layout

fn raw(n: usize, axes: Vec<f32>, down: &[usize]) -> RawPad {
    let mut p = RawPad { id: "Xbox Wireless Controller (Vendor: 045e Product: 0b13)".into(), index: 0, standard: false, buttons: vec![RawButton::default(); n], axes };
    for &d in down {
        p.press(d, true);
    }
    p
}

fn held(p: &RawPad) -> Vec<usize> {
    standard_view(p).0.iter().enumerate().filter(|(_, d)| **d).map(|(i, _)| i).collect()
}

#[test]
fn a_wired_xpad_pad_moves_back_start_guide_and_the_stick_clicks_to_their_slots() {
    assert_eq!(held(&raw(11, vec![0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0], &[6, 7, 8, 9, 10])), vec![8, 9, 10, 11, 16]);
    assert_eq!(held(&raw(11, vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, -1.0, 1.0], &[])), vec![6, 7, 13, 14]);
    assert_eq!(standard_view(&raw(11, vec![0.1, 0.2, -1.0, 0.3, 0.4, -1.0, 0.0, 0.0], &[])).1, vec![0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn a_bluetooth_pad_skips_the_gaps_and_takes_the_triggers_off_axes_5_and_4() {
    assert_eq!(held(&raw(15, vec![0.0, 0.0, 0.0, 0.0, -1.0, -1.0, 0.0, 0.0], &[0, 1, 3, 4, 6, 7, 10, 11, 12, 13, 14])), vec![0, 1, 2, 3, 4, 5, 8, 9, 10, 11, 16]);
    assert_eq!(held(&raw(15, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, -1.0], &[])), vec![6, 7, 12, 15]);
    assert_eq!(standard_view(&raw(15, vec![0.1, 0.2, 0.3, 0.4, -1.0, -1.0, 0.0, 0.0], &[])).1, vec![0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn a_resting_raw_pad_presses_nothing() {
    assert!(held(&raw(11, vec![0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0], &[])).is_empty());
    assert!(held(&raw(15, vec![0.0, 0.0, 0.0, 0.0, -1.0, -1.0, 0.0, 0.0], &[])).is_empty());
}

#[test]
fn a_pad_that_goes_away_lets_go_of_what_it_held_and_settles_its_look() {
    let mut gp = GamepadInput::default();
    let mut pad = fake_pad(vec![0.0, 0.0, 0.8, 0.0]);
    pad.press(0, true);
    let mut events = poll(&mut gp, 0.0, vec![Some(pad)]);
    assert!(gp.is_looking() && gp.is_held(Button::A));
    events.extend(poll(&mut gp, 16.0, vec![None]));
    assert!(!gp.connected && !gp.is_looking() && !gp.is_held(Button::A));
    assert!(events.contains(&PadEvent::Release { button: Button::A, t: 16.0, held: 16.0 }));
    assert_eq!(events.last(), Some(&PadEvent::Look { dx: 0.0, dy: 0.0, start: false }));
    assert!(poll(&mut gp, 32.0, vec![None]).is_empty());
}

// ---------------------------------------------------------------------------
// the left stick on four ways, the right stick on Turn

fn dirs(events: &[PadEvent]) -> Vec<Option<u8>> {
    events.iter().filter_map(|e| if let PadEvent::Dir { dir, .. } = e { Some(*dir) } else { None }).collect()
}

#[test]
fn four_ways_reads_a_forward_push_pulled_well_off_true_as_forward() {
    let mut gp = GamepadInput::default();
    gp.four_way = true;
    let mut e = poll(&mut gp, 0.0, vec![Some(fake_pad(vec![0.61f32.sin(), -0.61f32.cos(), 0.0, 0.0]))]);
    e.extend(poll(&mut gp, 16.0, vec![Some(fake_pad(vec![0.9, 0.05, 0.0, 0.0]))]));
    e.extend(poll(&mut gp, 32.0, vec![Some(fake_pad(vec![0.0; 4]))]));
    assert_eq!(dirs(&e), vec![Some(0), Some(2), None]);
}

#[test]
fn eight_ways_otherwise() {
    let mut gp = GamepadInput::default();
    assert_eq!(dirs(&poll(&mut gp, 0.0, vec![Some(fake_pad(vec![0.7, -0.7, 0.0, 0.0]))])), vec![Some(1)]);
}

fn not_settle(events: Vec<PadEvent>) -> Vec<PadEvent> {
    events.into_iter().filter(|e| !matches!(e, PadEvent::Look { dx, dy, .. } if *dx == 0.0 && *dy == 0.0)).collect()
}

#[test]
fn the_right_stick_on_turn_reads_left_and_right_as_the_left_stick_does() {
    let mut gp = GamepadInput::default();
    gp.right_stick_turns = true;
    let mut out = not_settle(poll(&mut gp, 0.0, vec![Some(fake_pad(vec![0.0, 0.0, 0.5, 0.0]))]));
    let push = vec![0.0, 0.0, 0.6 * 0.52f32.cos(), -0.6 * 0.52f32.sin()];
    out.extend(not_settle(poll(&mut gp, 16.0, vec![Some(fake_pad(push.clone()))])));
    out.extend(not_settle(poll(&mut gp, 400.0, vec![Some(fake_pad(push))])));
    out.extend(not_settle(poll(&mut gp, 416.0, vec![Some(fake_pad(vec![0.0; 4]))])));
    assert_eq!(out, vec![PadEvent::Dir { source: DirSource::RStick, dir: Some(2) }, PadEvent::DirRepeat { source: DirSource::RStick, dir: 2, n: 1 }, PadEvent::Dir { source: DirSource::RStick, dir: None }]);
}

#[test]
fn the_right_stick_on_turn_pushed_up_or_down_only_tilts() {
    let mut gp = GamepadInput::default();
    gp.right_stick_turns = true;
    let out = not_settle(poll(&mut gp, 0.0, vec![Some(fake_pad(vec![0.0, 0.0, 0.2, -0.9]))]));
    assert_eq!(out.len(), 1);
    assert!(matches!(out[0], PadEvent::Look { dx, dy, start: true } if dx == 0.0 && dy < 0.0));
}

#[test]
fn free_look_sends_no_directions() {
    let mut gp = GamepadInput::default();
    let out = not_settle(poll(&mut gp, 0.0, vec![Some(fake_pad(vec![0.0, 0.0, 0.9, 0.0]))]));
    assert_eq!(out.len(), 1);
    assert!(matches!(out[0], PadEvent::Look { dy, start: true, .. } if dy == 0.0));
}

// ---------------------------------------------------------------------------
// gamepad-repeat.test.ts

#[test]
fn a_held_button_repeats_at_the_keyboards_cadence() {
    let mut gp = GamepadInput::default();
    let mut rt = fake_pad(vec![0.0; 4]);
    rt.press(7, true);
    assert_eq!(poll(&mut gp, 0.0, vec![Some(rt.clone())]), vec![PadEvent::Press { button: Button::RT, t: 0.0 }]);
    let mut at = Vec::new();
    for now in 1..=1000 {
        for e in poll(&mut gp, now as f64, vec![Some(rt.clone())]) {
            if matches!(e, PadEvent::Repeat { .. }) {
                at.push(now);
            }
        }
    }
    assert_eq!(at, vec![420, 520, 620, 720, 820, 920]);
    let e = poll(&mut gp, 1036.0, vec![Some(fake_pad(vec![0.0; 4]))]);
    assert!(matches!(e.last(), Some(PadEvent::Release { button: Button::RT, .. })));
    assert!(!poll(&mut gp, 1136.0, vec![Some(fake_pad(vec![0.0; 4]))]).iter().any(|e| matches!(e, PadEvent::Repeat { .. })));
}

// ---------------------------------------------------------------------------
// pad-keys.test.ts: keyboard stand-ins for the pad

fn f(code: &str, shift: bool) -> KeyLike {
    KeyLike { key: code.into(), code: code.into(), shift, ..Default::default() }
}

#[test]
fn presses_and_releases_rb_on_shift_f1() {
    let mut pad = GamepadInput::default();
    assert!(pad_key_input(&mut pad, &KeyInput::Down { key: f("F1", true), repeat: false }, 0.0));
    assert!(matches!(pad.take_events()[..], [PadEvent::Press { button: Button::RB, .. }]));
    assert!(pad.is_held(Button::RB));
    pad_key_input(&mut pad, &KeyInput::Up { key: f("F1", false) }, 10.0);
    assert!(matches!(pad.take_events()[..], [PadEvent::Release { button: Button::RB, .. }]));
    assert!(!pad.is_held(Button::RB));
}

#[test]
fn holds_through_the_key_repeat() {
    let mut pad = GamepadInput::default();
    pad_key_input(&mut pad, &KeyInput::Down { key: f("F1", true), repeat: false }, 0.0);
    pad_key_input(&mut pad, &KeyInput::Down { key: f("F1", true), repeat: true }, 1.0);
    pad_key_input(&mut pad, &KeyInput::Down { key: f("F1", true), repeat: true }, 2.0);
    assert_eq!(pad.take_events().iter().filter(|e| matches!(e, PadEvent::Press { .. })).count(), 1);
}

#[test]
fn leaves_crawls_punctuation_a_bare_f1_and_the_other_keys_alone() {
    let mut pad = GamepadInput::default();
    assert!(!pad_key_input(&mut pad, &KeyInput::Down { key: f("Backquote", true), repeat: false }, 0.0));
    assert!(!pad_key_input(&mut pad, &KeyInput::Down { key: f("F1", false), repeat: false }, 0.0));
    assert!(!pad_key_input(&mut pad, &KeyInput::Down { key: f("F3", true), repeat: false }, 0.0));
    assert!(pad.take_events().is_empty());
}

#[test]
fn lets_go_when_the_window_loses_focus() {
    let mut pad = GamepadInput::default();
    pad_key_input(&mut pad, &KeyInput::Down { key: f("F2", true), repeat: false }, 0.0);
    assert!(pad.is_held(Button::Select));
    pad.virtual_release(5.0);
    assert!(!pad.is_held(Button::Select));
    assert!(matches!(pad.take_events()[1], PadEvent::Release { button: Button::Select, .. }));
}

#[test]
fn a_scripted_pad_plays_its_steps() {
    let mut script = ScriptedPad::parse("0:;100:A;200:;300::0,-1,0,0");
    let mut gp = GamepadInput::default();
    let mut events = Vec::new();
    for t in [0.0, 100.0, 200.0, 300.0] {
        script.now = t;
        gp.poll_source(t, &mut script);
        events.extend(gp.take_events());
    }
    assert_eq!(events, vec![PadEvent::Press { button: Button::A, t: 100.0 }, PadEvent::Release { button: Button::A, t: 200.0, held: 100.0 }, PadEvent::Dir { source: DirSource::LStick, dir: Some(0) }]);
    assert!(script.done());
}

// ---------------------------------------------------------------------------
// gamepad-hints.test.ts

#[derive(Clone, Default)]
struct Store(Rc<RefCell<Option<String>>>, Rc<RefCell<bool>>);

impl HintStore for Store {
    fn get(&self) -> Option<String> {
        self.0.borrow().clone()
    }
    fn set(&mut self, value: &str) {
        if !*self.1.borrow() {
            *self.0.borrow_mut() = Some(value.into());
        }
    }
}

fn ctx() -> Context {
    Context::default()
}
fn evidence() -> PadHintEvidence {
    PadHintEvidence::default()
}
fn keys(t: &str) -> Action {
    Action::keys(vec![SeqItem::text(t)], t)
}
fn teaching(h: &GamepadHints, c: &Context) -> Vec<(Button, String)> {
    h.prompts(c, HintMode::Adaptive).into_iter().filter(|l| l.teaching).map(|l| (l.button, l.label)).collect()
}
fn buttons(v: Vec<(Button, String)>) -> Vec<Button> {
    v.into_iter().map(|(b, _)| b).collect()
}
fn basics(h: &mut GamepadHints) {
    h.moved(3);
    h.looked_by(0.4);
}
fn hints(store: &Store) -> GamepadHints {
    GamepadHints::with_store(Box::new(store.clone()))
}

#[test]
fn starts_with_two_basic_actions_not_the_entire_binding_table() {
    let store = Store::default();
    let mut h = hints(&store);
    assert_eq!(teaching(&h, &ctx()), vec![(Button::LStick, "Move".into()), (Button::RStick, "Look around".into())]);
    h.moved(1);
    h.moved(1);
    assert!(!h.knows(PadLesson::Move));
    h.moved(1);
    assert_eq!(buttons(teaching(&h, &ctx())), vec![Button::RStick]);
    h.looked_by(0.01);
    assert!(!h.knows(PadLesson::Look));
    h.looked_by(0.3);
    assert_eq!(buttons(teaching(&h, &ctx())), vec![Button::R3]);
}

#[test]
fn teaches_examine_on_r3_and_wait_rest_on_lb_never_b() {
    let mut h = hints(&Store::default());
    basics(&mut h);
    assert_eq!(teaching(&h, &ctx()), vec![(Button::R3, "Examine".into())]);
    let t = binding_table(&ctx());
    h.attempt(&t[Button::R3], &ctx(), evidence(), 0.0);
    h.observe(&PadHintEvidence { mode: Mode::Targeting, ..evidence() }, 10.0);
    h.attempt(&t[Button::Select], &ctx(), evidence(), 20.0);
    h.observe(&PadHintEvidence { client_overlay: true, ..evidence() }, 30.0);
    let l = h.prompts(&ctx(), HintMode::Adaptive).into_iter().find(|l| l.teaching).unwrap();
    assert_eq!((l.button, l.label.as_str(), l.hold.as_deref()), (Button::LB, "Wait one turn", Some("Rest")));
    assert_eq!(pad_lesson(&t[Button::B], &ctx()), None);
}

#[test]
fn remembers_learned_actions_across_screens_and_runs() {
    let store = Store::default();
    let mut h = hints(&store);
    basics(&mut h);
    let mut next = hints(&store);
    assert!(next.knows(PadLesson::Move) && next.knows(PadLesson::Look));
    assert_eq!(buttons(teaching(&next, &ctx())), vec![Button::R3]);
    next.reset();
    assert_eq!(buttons(teaching(&hints(&store), &ctx())), vec![Button::LStick, Button::RStick]);
}

#[test]
fn never_retires_contextual_interactions_and_teaches_only_autofight_in_combat() {
    let mut h = hints(&Store::default());
    basics(&mut h);
    let door = Context { ahead: Target::DoorClosed { label: "door".into() }, ..ctx() };
    assert_eq!(h.prompts(&door, HintMode::Adaptive).iter().find(|l| l.button == Button::A).map(|l| l.label.clone()), Some("Open door".into()));
    assert_eq!(h.prompts(&door, HintMode::Contextual).iter().map(|l| l.label.clone()).collect::<Vec<_>>(), vec!["Open door"]);
    assert!(h.prompts(&door, HintMode::Off).is_empty());
    assert_eq!(teaching(&h, &Context { hostiles_in_view: 1, ..ctx() }), vec![(Button::RT, "Autofight".into())]);
    assert_eq!(buttons(teaching(&h, &ctx())), vec![Button::R3]);
}

#[test]
fn does_not_teach_in_spectating_lobby_ended_macro_text() {
    for mode in [Mode::Spectating, Mode::Lobby, Mode::Ended, Mode::Macro, Mode::Text] {
        assert!(hints(&Store::default()).prompts(&Context { mode, ..ctx() }, HintMode::Adaptive).is_empty());
    }
}

#[test]
fn keeps_targeting_confirmation_even_when_hints_are_off() {
    let h = hints(&Store::default());
    let targeting = Context { mode: Mode::Targeting, ..ctx() };
    assert_eq!(h.prompts(&targeting, HintMode::Off).into_iter().map(|l| (l.button, l.label)).collect::<Vec<_>>(), vec![(Button::A, "Fire".into())]);
    assert_eq!(teaching(&h, &targeting).into_iter().map(|t| t.1).collect::<Vec<_>>(), vec!["Move cursor"]);
}

#[test]
fn the_level_map_stands_all_its_keys_in_the_corner() {
    let h = hints(&Store::default());
    let map = Context { mode: Mode::Levelmap, ..ctx() };
    let got: Vec<(&str, String)> = h.prompts(&map, HintMode::Contextual).into_iter().map(|l| (l.button.name(), l.label)).collect();
    let want = [("X", "Describe"), ("Y", "Find you"), ("LB", "Up stairs"), ("RB", "Down stairs"), ("LT", "Zoom out"), ("RT", "Zoom in"), ("L3", "Find…"), ("R3", "Overview"), ("A", "Travel here")];
    assert_eq!(got, want.iter().map(|(b, l)| (*b, l.to_string())).collect::<Vec<_>>());
    assert_eq!(h.prompts(&map, HintMode::Off).into_iter().map(|l| l.label).collect::<Vec<_>>(), vec!["Travel here"]);
    assert_eq!(teaching(&h, &map).into_iter().map(|t| t.1).collect::<Vec<_>>(), vec!["Move cursor"]);
}

#[test]
fn never_advertises_b_regardless_of_hint_setting() {
    for mode in [Mode::Command, Mode::Menu, Mode::Targeting, Mode::Levelmap, Mode::Popup, Mode::Newgame, Mode::Crt, Mode::Dialog, Mode::More, Mode::Prompt, Mode::Yesno, Mode::Text, Mode::Spectating, Mode::Lobby, Mode::Ended, Mode::Macro] {
        let h = hints(&Store::default());
        let c = Context { mode, focus: Some(FocusInfo { label: Some("Yes".into()), cancel_label: Some("No".into()), index: 0, count: 2, ..Default::default() }), ..ctx() };
        for setting in [HintMode::Adaptive, HintMode::Contextual, HintMode::Off] {
            assert!(!h.prompts(&c, setting).iter().any(|l| l.button == Button::B), "{mode:?} {setting:?}");
        }
    }
}

#[test]
fn learns_actions_by_meaning_and_context_not_button_usage() {
    let mut h = hints(&Store::default());
    let menu = Context { mode: Mode::Menu, ..ctx() };
    assert_eq!(pad_lesson(&binding_table(&menu)[Button::B], &menu), None);
    h.attempt(&keys("."), &ctx(), evidence(), 0.0);
    h.observe(&PadHintEvidence { turn: 2, ..evidence() }, 10.0);
    assert!(h.knows(PadLesson::Wait) && !h.knows(PadLesson::Rest));
    h.attempt(&keys("5"), &ctx(), PadHintEvidence { turn: 2, ..evidence() }, 20.0);
    h.observe(&PadHintEvidence { turn: 3, ..evidence() }, 30.0);
    assert!(h.knows(PadLesson::Rest) && !h.knows(PadLesson::Navigate));
}

#[test]
fn never_adds_explore_actions_or_equipment_on_their_own() {
    let mut h = hints(&Store::default());
    let standing = |h: &GamepadHints| h.prompts(&ctx(), HintMode::Adaptive).into_iter().filter(|l| !l.contextual && !l.teaching).count();
    assert_eq!(standing(&h), 0);
    basics(&mut h);
    h.attempt(&Action::ui(UiOp::Commands), &ctx(), evidence(), 0.0);
    h.observe(&PadHintEvidence { client_overlay: true, ..evidence() }, 10.0);
    h.attempt(&Action::ui(UiOp::Equipment), &ctx(), evidence(), 20.0);
    h.observe(&PadHintEvidence { mode: Mode::Menu, ..evidence() }, 30.0);
    h.attempt(&keys("o"), &ctx(), evidence(), 40.0);
    h.observe(&PadHintEvidence { x: 11, ..evidence() }, 50.0);
    for id in [PadLesson::Commands, PadLesson::Equipment, PadLesson::Explore] {
        assert!(h.knows(id));
    }
    assert_eq!(standing(&h), 0);
    assert!(h.prompts(&ctx(), HintMode::Adaptive).iter().all(|l| l.teaching));
}

#[test]
fn keeps_contextual_actions_first_then_the_lessons() {
    let h = hints(&Store::default());
    let door = Context { ahead: Target::DoorClosed { label: "door".into() }, ..ctx() };
    let at_door = h.prompts(&door, HintMode::Adaptive);
    assert_eq!(at_door[0].label, "Open door");
    assert!(at_door[1..].iter().all(|l| l.teaching));
    let combat = Context { hostiles_in_view: 1, ahead: Target::Monster { id: None, hostile: true, label: "orc".into() }, ..ctx() };
    let prompts = h.prompts(&combat, HintMode::Adaptive);
    assert!(prompts.iter().any(|l| l.label == "Attack orc"));
    assert!(prompts.iter().all(|l| l.contextual || l.teaching));
    let mut seen: Vec<Button> = prompts.iter().map(|l| l.button).collect();
    seen.dedup();
    assert_eq!(seen.len(), prompts.len());
}

#[test]
fn retires_the_autofight_lesson_once_a_fight_has_taken_a_turn() {
    let mut h = hints(&Store::default());
    let combat = Context { hostiles_in_view: 1, ..ctx() };
    assert_eq!(buttons(teaching(&h, &combat)), vec![Button::RT]);
    h.attempt(&Action::Fight, &combat, evidence(), 0.0);
    h.observe(&PadHintEvidence { turn: 2, ..evidence() }, 100.0);
    assert!(h.knows(PadLesson::Fight));
    assert!(teaching(&h, &combat).is_empty());
    assert!(h.prompts(&combat, HintMode::Contextual).is_empty());
}

#[test]
fn shows_nothing_in_contextual_or_off_with_nothing_in_front() {
    let h = hints(&Store::default());
    assert!(h.prompts(&ctx(), HintMode::Contextual).is_empty());
    assert!(h.prompts(&ctx(), HintMode::Off).is_empty());
}

#[test]
fn only_learns_after_the_outcome() {
    let cases: Vec<(PadLesson, Action, PadHintEvidence)> = vec![
        (PadLesson::Commands, Action::ui(UiOp::Commands), PadHintEvidence { mode: Mode::Menu, ..evidence() }),
        (PadLesson::Commands, Action::ui(UiOp::Commands), PadHintEvidence { client_overlay: true, ..evidence() }),
        (PadLesson::Travel, Action::ui(UiOp::Levelmap), PadHintEvidence { mode: Mode::Levelmap, ..evidence() }),
        (PadLesson::Equipment, Action::ui(UiOp::Equipment), PadHintEvidence { mode: Mode::Menu, ..evidence() }),
        (PadLesson::Examine, Action::Examine, PadHintEvidence { mode: Mode::Targeting, ..evidence() }),
        (PadLesson::Explore, keys("o"), PadHintEvidence { x: 11, ..evidence() }),
        (PadLesson::Wait, keys("."), PadHintEvidence { turn: 2, ..evidence() }),
        (PadLesson::Rest, keys("5"), PadHintEvidence { turn: 2, ..evidence() }),
        (PadLesson::Fight, Action::Fight, PadHintEvidence { turn: 2, ..evidence() }),
    ];
    for (id, a, outcome) in cases {
        let mut h = hints(&Store::default());
        h.attempt(&a, &ctx(), evidence(), 0.0);
        h.observe(&evidence(), 10.0);
        assert!(!h.knows(id), "{id:?}");
        h.observe(&outcome, 100.0);
        assert!(h.knows(id), "{id:?}");
    }
}

#[test]
fn does_not_mistake_a_refusal_or_a_more_for_a_menu_that_opened() {
    let mut h = hints(&Store::default());
    h.attempt(&Action::ui(UiOp::Equipment), &ctx(), evidence(), 0.0);
    h.observe(&PadHintEvidence { mode: Mode::More, ..evidence() }, 10.0);
    h.observe(&PadHintEvidence { mode: Mode::Menu, ..evidence() }, 2000.0);
    assert!(!h.knows(PadLesson::Equipment));
}

#[test]
fn switching_devices_or_another_action_cancels_pending_attribution() {
    let mut h = hints(&Store::default());
    h.attempt(&Action::ui(UiOp::Equipment), &ctx(), evidence(), 0.0);
    h.cancel();
    h.observe(&PadHintEvidence { mode: Mode::Menu, ..evidence() }, 10.0);
    assert!(!h.knows(PadLesson::Equipment));
    h.attempt(&keys("o"), &ctx(), evidence(), 20.0);
    h.attempt(&Action::step(0), &ctx(), evidence(), 21.0);
    h.observe(&PadHintEvidence { x: 11, ..evidence() }, 30.0);
    assert!(!h.knows(PadLesson::Explore));
}

#[test]
fn learns_navigation_separately_and_only_when_focus_moves() {
    let mut h = hints(&Store::default());
    let c = Context { mode: Mode::Menu, ..ctx() };
    let b = PadHintEvidence { mode: Mode::Menu, focus: 0, ..evidence() };
    h.attempt(&Action::menu(isorun_input::bindings::MenuOp::Next), &c, b.clone(), 0.0);
    h.observe(&b, 10.0);
    assert!(!h.knows(PadLesson::Navigate));
    h.attempt(&Action::menu(isorun_input::bindings::MenuOp::Next), &c, b.clone(), 20.0);
    h.observe(&PadHintEvidence { focus: 1, ..b }, 30.0);
    assert!(h.knows(PadLesson::Navigate) && !h.knows(PadLesson::Move));
    let _ = FocusOp::Next;
}

#[test]
fn moving_a_targeting_cursor_is_not_walking_or_learning_the_level_map() {
    let mut h = hints(&Store::default());
    let c = Context { mode: Mode::Targeting, ..ctx() };
    let b = PadHintEvidence { mode: Mode::Targeting, cursor: "1,1".into(), ..evidence() };
    h.attempt(&Action::Cursor { dir: 2 }, &c, b.clone(), 0.0);
    h.observe(&PadHintEvidence { cursor: "2,1".into(), ..b }, 10.0);
    assert!(h.knows(PadLesson::TargetCursor) && !h.knows(PadLesson::Move) && !h.knows(PadLesson::MapCursor));
}

#[test]
fn ignores_malformed_storage_and_tolerates_blocked_writes() {
    let store = Store::default();
    *store.0.borrow_mut() = Some("{broken".into());
    assert!(!hints(&store).knows(PadLesson::Move));
    *store.0.borrow_mut() = Some(r#"["look","not-a-lesson",4]"#.into());
    let mut h = hints(&store);
    assert!(h.knows(PadLesson::Look));
    *store.1.borrow_mut() = true;
    h.moved(3);
    assert!(h.knows(PadLesson::Move));
}

#[test]
fn only_deliberate_pad_events_activate_hints() {
    assert!(!is_pad_activity(&PadEvent::Look { dx: 0.0, dy: 0.0, start: false }));
    assert!(!is_pad_activity(&PadEvent::Dir { source: DirSource::LStick, dir: None }));
    assert!(!is_pad_activity(&PadEvent::Release { button: Button::A, t: 100.0, held: 100.0 }));
    assert!(is_pad_activity(&PadEvent::Press { button: Button::A, t: 0.0 }));
    assert!(is_pad_activity(&PadEvent::Dir { source: DirSource::LStick, dir: Some(6) }));
    assert!(is_pad_activity(&PadEvent::Look { dx: 0.5, dy: 0.0, start: true }));
    assert!(!is_pad_activity(&PadEvent::Look { dx: 0.5, dy: 0.0, start: false }));
    assert!(!is_pad_activity(&PadEvent::DirRepeat { source: DirSource::LStick, dir: 6, n: 1 }));
    assert!(!is_pad_activity(&PadEvent::Repeat { button: Button::A, n: 1 }));
}
