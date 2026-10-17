//! The port of `osk-cursor.test.ts`, the touch bar's labels, the gesture recogniser (game.ts's
//! pointer handlers) and the routing of keys and pad events (game.ts `onKeyDown`, `pad`).

use isorun_input::bindings::{Action, OskOp, UiOp};
use isorun_input::context::{Context, Mode, Target};
use isorun_input::gamepad::{Button, DirSource, PadEvent};
use isorun_input::keys::KeyLike;
use isorun_input::osk::*;
use isorun_input::router::*;
use isorun_input::runner::{Env, Runner, RunnerHooks};
use isorun_input::touch::*;
use isorun_input::touch_bar::{shown_labels, touch_labels};
use isorun_render::camera::{CameraAngles, IsoCamera};
use isorun_scene::Scene;
use isorun_webtiles::{cm, initial_state, GameState, MouseMode};
use serde_json::Value;

// ---------------------------------------------------------------------------
// osk-cursor.test.ts

fn extras() -> OskTarget {
    OskTarget { extras: ['<', '>', '$', '^'].iter().zip(["up", "down", "last", "here"]).map(|(c, l)| OskExtra { ch: *c, label: l.into() }).collect(), submit_button: None }
}

#[test]
fn the_keyboard_starts_on_the_first_key_each_time_it_is_shown() {
    let mut osk = Osk::default();
    osk.attach(OskTarget::default());
    for _ in 0..5 {
        osk.op(OskOp::Move, Some(2));
    }
    osk.detach();
    osk.attach(extras());
    assert_eq!(osk.op(OskOp::Type, None), Some(OskEdit::Type('<')));
}

#[test]
fn stays_on_a_key_when_a_rebuild_swaps_in_a_target_with_fewer_rows() {
    let mut osk = Osk::default();
    osk.attach(extras());
    // up from the extras row wraps to the last of seven rows
    osk.op(OskOp::Move, Some(0));
    osk.attach(OskTarget::default());
    assert!(matches!(osk.op(OskOp::Type, None), Some(OskEdit::Type(_))));
}

#[test]
fn stays_on_a_key_when_a_rebuild_swaps_in_a_target_whose_first_row_is_shorter() {
    let mut osk = Osk::default();
    osk.attach(OskTarget::default());
    for _ in 0..6 {
        osk.op(OskOp::Move, Some(2));
    }
    osk.attach(extras());
    assert_eq!(osk.op(OskOp::Type, None), Some(OskEdit::Type('^')));
}

#[test]
fn shift_types_capitals_backspace_and_space_edit_the_field() {
    let mut osk = Osk::default();
    osk.attach(OskTarget::default());
    let mut field = OskText::new("");
    osk.op(OskOp::Move, Some(4));
    osk.op(OskOp::Shift, None);
    for op in [OskOp::Type, OskOp::Space, OskOp::Type, OskOp::Backspace] {
        if let Some(e) = osk.op(op, None) {
            field.apply(&e);
        }
    }
    assert_eq!(field.text, "Q ");
    assert_eq!(osk.op(OskOp::Submit, None), Some(OskEdit::Submit));
    assert_eq!(OskText { password: true, ..OskText::new("abc") }.preview(), "•••");
}

// ---------------------------------------------------------------------------
// the touch bar

#[test]
fn the_touch_bar_shows_each_label_once_b_first_a_last_and_not_in_command_mode() {
    let menu = Context { mode: Mode::Menu, ..Default::default() };
    let labels: Vec<(Button, String)> = touch_labels(&menu).into_iter().map(|l| (l.button, l.label)).collect();
    assert_eq!(labels.first().map(|l| l.0), Some(Button::B));
    assert_eq!(labels.last().map(|l| l.0), Some(Button::A));
    assert!(shown_labels(&Context::default(), true).is_empty());
    assert!(shown_labels(&menu, false).is_empty());
    let more = Context { mode: Mode::More, ..Default::default() };
    assert_eq!(touch_labels(&more).into_iter().map(|l| l.label).collect::<Vec<_>>(), vec!["Skip", "--more--"]);
}

// ---------------------------------------------------------------------------
// gestures

fn touch(id: u64, phase: egui::TouchPhase, x: f32, y: f32) -> egui::Event {
    egui::Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(id), phase, pos: egui::pos2(x, y), force: None }
}

fn kinds(g: &[Gesture]) -> Vec<String> {
    g.iter()
        .filter(|g| !matches!(g, Gesture::Pressed(_)))
        .map(|g| match g {
            Gesture::TouchHover(Some(_)) => "hover".into(),
            Gesture::TouchHover(None) => "unhover".into(),
            Gesture::Click { button, .. } => format!("click{button}"),
            Gesture::LongPress { .. } => "long".into(),
            Gesture::Drag { .. } => "drag".into(),
            Gesture::Zoom(_) => "zoom".into(),
            Gesture::Turn(_) => "turn".into(),
            Gesture::Tilt(_) => "tilt".into(),
            Gesture::SnapTurn => "snap".into(),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn a_tap_hovers_its_cell_clicks_it_and_keeps_the_cursor_a_moment() {
    let mut g = Gestures::default();
    let mut out = g.feed(&[touch(1, egui::TouchPhase::Start, 100.0, 100.0)], 0.0);
    out.extend(g.feed(&[touch(1, egui::TouchPhase::End, 101.0, 100.0)], 100.0));
    assert_eq!(kinds(&out), vec!["hover", "click1"]);
    assert!(g.feed(&[], 300.0).is_empty());
    assert_eq!(kinds(&g.feed(&[], 700.0)), vec!["unhover"]);
}

#[test]
fn a_finger_held_still_is_a_long_press_and_no_click() {
    let mut g = Gestures::default();
    g.feed(&[touch(1, egui::TouchPhase::Start, 100.0, 100.0)], 0.0);
    assert_eq!(kinds(&g.feed(&[], LONG_PRESS_MS + 1.0)), vec!["long"]);
    assert!(kinds(&g.feed(&[touch(1, egui::TouchPhase::End, 100.0, 100.0)], 600.0)).iter().all(|k| k != "click1"));
}

#[test]
fn a_finger_that_moves_past_the_slop_drags_the_view() {
    let mut g = Gestures::default();
    g.feed(&[touch(1, egui::TouchPhase::Start, 100.0, 100.0)], 0.0);
    let out = g.feed(&[touch(1, egui::TouchPhase::Move, 120.0, 100.0)], 50.0);
    assert_eq!(kinds(&out), vec!["unhover", "drag"]);
    assert!(kinds(&g.feed(&[touch(1, egui::TouchPhase::End, 120.0, 100.0)], 100.0)).iter().all(|k| k != "click1"));
}

#[test]
fn two_fingers_zoom_turn_and_tilt_and_the_turn_ends_on_a_step() {
    let mut g = Gestures::default();
    g.feed(&[touch(1, egui::TouchPhase::Start, 100.0, 100.0), touch(2, egui::TouchPhase::Start, 200.0, 100.0)], 0.0);
    let out = g.feed(&[touch(2, egui::TouchPhase::Move, 260.0, 120.0)], 50.0);
    assert!(out.iter().any(|g| matches!(g, Gesture::Zoom(f) if *f > 1.0)));
    assert!(out.iter().any(|g| matches!(g, Gesture::Turn(_))));
    assert!(out.iter().any(|g| matches!(g, Gesture::Tilt(t) if *t > 0.0)));
    assert!(kinds(&g.feed(&[touch(2, egui::TouchPhase::End, 260.0, 120.0)], 80.0)).contains(&"snap".to_string()));
}

#[test]
fn the_wheel_zooms_and_with_shift_turns_a_step() {
    let mut g = Gestures::default();
    let zoom = g.feed(&[egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, 1.0), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: egui::Modifiers::default() }], 0.0);
    assert!(matches!(zoom[..], [Gesture::Zoom(f)] if f > 1.0));
    let turn = g.feed(&[egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: egui::vec2(0.0, -1.0), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: egui::Modifiers { shift: true, ..Default::default() } }], 0.0);
    assert!(matches!(turn[..], [Gesture::Turn(d)] if d == 45.0));
}

#[test]
fn compact_and_landscape_windows() {
    assert!(compact_window(egui::vec2(800.0, 600.0)));
    assert!(compact_window(egui::vec2(1600.0, 500.0)));
    assert!(!compact_window(egui::vec2(1600.0, 1000.0)));
    assert!(landscape_window(egui::vec2(1600.0, 1000.0)));
}

// ---------------------------------------------------------------------------
// the routing

#[derive(Default)]
struct Host {
    ui: Vec<UiOp>,
    overlay: bool,
    overlay_ops: Vec<OverlayOp>,
    aim: Vec<Option<u8>>,
    system: u32,
}

impl RunnerHooks for Host {
    fn ui(&mut self, op: UiOp, _a: Option<i32>, _c: Option<isorun_input::bindings::CommandCategory>, _s: Option<&str>) {
        self.ui.push(op);
        if op == UiOp::System || op == UiOp::Commands {
            self.overlay = true;
        }
    }
}

impl GameHost for Host {
    fn client_overlay(&self) -> bool {
        self.overlay
    }
    fn client_overlay_input(&mut self, op: OverlayOp) -> bool {
        self.overlay_ops.push(op);
        if matches!(op, OverlayOp::Close | OverlayOp::Cancel) {
            self.overlay = false;
        }
        true
    }
    fn set_aim(&mut self, dir: Option<u8>) {
        self.aim.push(dir);
    }
    fn show_system(&mut self, _spectating: bool) {
        self.system += 1;
    }
}

struct R {
    router: InputRouter,
    runner: Runner,
    st: GameState,
    scene: Scene,
    ctx: Context,
    cam: IsoCamera,
    host: Host,
    now: f64,
    sent: Vec<Value>,
}

impl R {
    fn new(facing: i32, settings: RouterSettings) -> Self {
        let mut st = initial_state();
        st.input_mode = MouseMode::COMMAND;
        R { router: InputRouter::new(settings), runner: Runner::new(), st, scene: Scene { player: (5, 5), player_on_level: true, ..Default::default() }, ctx: Context::default(), cam: IsoCamera::new(CameraAngles { rotation: facing as f32 * 45.0, tilt: 0.0, perspective: 0.0 }, 68.0), host: Host::default(), now: 0.0, sent: Vec::new() }
    }

    fn io<T>(&mut self, f: impl FnOnce(&mut InputRouter, &mut Io) -> T) -> T {
        let env = Env { state: &self.st, scene: &self.scene, ctx: &self.ctx, camera: &mut self.cam, now: self.now, watching: false, confirm_dangerous: false };
        let mut io = Io { runner: &mut self.runner, env, host: &mut self.host };
        let out = f(&mut self.router, &mut io);
        self.sent.extend(self.runner.take_outbox());
        out
    }

    fn key(&mut self, key: &str) -> bool {
        let k = KeyLike { key: key.into(), ..Default::default() };
        self.io(|r, io| r.key_down(&k, false, io))
    }

    fn key_up(&mut self, key: &str) {
        let k = KeyLike { key: key.into(), ..Default::default() };
        self.io(|r, io| r.key_up(&k, io));
    }

    fn pad(&mut self, ev: PadEvent) {
        self.io(|r, io| r.pad(&ev, io));
    }

    fn texts(&self) -> Vec<String> {
        self.sent.iter().map(|m| m["text"].as_str().map(String::from).unwrap_or_else(|| format!("^{}", m["keycode"]))).collect()
    }
}

fn step_keys() -> RouterSettings {
    RouterSettings { arrow_keys_aim: false, left_stick_aim: false, explore_on_hold: false }
}

#[test]
fn vim_keys_step_relative_to_the_screen() {
    // facing east: `k` (up the screen) is east, `l` (right) south
    let mut r = R::new(2, step_keys());
    r.key("k");
    r.key("l");
    r.key("o");
    assert_eq!(r.texts(), vec!["l", "j", "o"]);
}

#[test]
fn the_arrows_aim_and_space_steps_that_way_after_the_camera_turns() {
    let mut r = R::new(0, RouterSettings::default());
    r.key("ArrowUp");
    assert_eq!(r.router.aim_dir, Some(0));
    r.key(" ");
    r.key_up("ArrowUp");
    assert_eq!(r.router.aim_dir, None);
    // the camera turns a step to the right: up the screen is north-east now, and up and right east
    r.cam.turn(45.0);
    r.key("ArrowUp");
    r.key("ArrowRight");
    assert_eq!(r.router.aim_dir, Some(2));
    r.key("Enter");
    r.key("Escape");
    assert_eq!(r.router.aim_dir, None);
    assert_eq!(r.texts(), vec!["k", "l"]);
}

#[test]
fn escape_in_command_mode_opens_the_isorun_menu_and_closes_it_again() {
    let mut r = R::new(0, RouterSettings::default());
    r.key("Escape");
    assert_eq!(r.host.ui, vec![UiOp::System]);
    assert!(r.host.overlay);
    r.key("Escape");
    assert_eq!(r.host.overlay_ops, vec![OverlayOp::Cancel]);
    assert!(r.sent.is_empty());
}

#[test]
fn the_pad_drives_the_runner_lt_tapped_explores_rt_autofights_a_steps_on_the_aim() {
    let mut r = R::new(0, RouterSettings::default());
    r.ctx.explore_on_hold = false;
    // LT tapped: autoexplore
    r.pad(PadEvent::Press { button: Button::LT, t: 0.0 });
    r.pad(PadEvent::Release { button: Button::LT, t: 100.0, held: 100.0 });
    // RT: autofight, on press
    r.pad(PadEvent::Press { button: Button::RT, t: 200.0 });
    // the left stick on Aim points, A steps that way
    r.pad(PadEvent::Dir { source: DirSource::LStick, dir: Some(2) });
    r.pad(PadEvent::Press { button: Button::A, t: 300.0 });
    r.pad(PadEvent::Release { button: Button::A, t: 350.0, held: 50.0 });
    assert_eq!(r.texts(), vec!["o", "^9", "l"]);
}

#[test]
fn lb_held_past_hold_ms_rests_and_its_release_is_no_tap() {
    let mut r = R::new(0, RouterSettings::default());
    r.pad(PadEvent::Press { button: Button::LB, t: 0.0 });
    r.now = 500.0;
    r.io(|router, io| {
        router.fire_holds(io, &|b| b == Button::LB);
    });
    r.pad(PadEvent::Release { button: Button::LB, t: 600.0, held: 600.0 });
    assert_eq!(r.texts(), vec!["5"]);
}

#[test]
fn lt_on_hold_explores_while_held_and_stops_when_let_go() {
    let mut r = R::new(0, RouterSettings::default());
    r.ctx.explore_on_hold = true;
    r.pad(PadEvent::Press { button: Button::LT, t: 0.0 });
    r.now = 500.0;
    r.io(|router, io| router.fire_holds(io, &|b| b == Button::LT));
    r.pad(PadEvent::Release { button: Button::LT, t: 600.0, held: 600.0 });
    assert_eq!(r.texts(), vec!["o", "^27"]);
}

#[test]
fn the_button_that_opened_a_screen_puts_it_away() {
    let mut r = R::new(0, RouterSettings::default());
    r.pad(PadEvent::Press { button: Button::Start, t: 0.0 });
    assert!(r.host.overlay);
    r.pad(PadEvent::Press { button: Button::Start, t: 100.0 });
    assert_eq!(r.host.overlay_ops, vec![OverlayOp::Close]);
    assert!(!r.host.overlay);
}

#[test]
fn on_a_screen_the_keyboard_is_a_pad() {
    let mut r = R::new(0, RouterSettings::default());
    r.ctx = Context { mode: Mode::More, ..Default::default() };
    r.key("Enter");
    assert_eq!(r.sent, vec![cm::key(32)]);
}

#[test]
fn the_back_button_cancels_what_is_up() {
    let mut r = R::new(0, RouterSettings::default());
    r.ctx = Context { mode: Mode::Targeting, ahead: Target::none("floor"), ..Default::default() };
    r.io(|router, io| router.back(io));
    assert_eq!(r.sent, vec![cm::key(27)]);
    let mut t = R::new(0, RouterSettings::default());
    t.ctx = Context { mode: Mode::Text, ..Default::default() };
    t.io(|router, io| router.back(io));
    assert_eq!(t.sent, vec![cm::key(27)]);
    let _ = Action::osk(OskOp::Cancel);
}

#[test]
fn a_click_on_the_view_is_crawls_click_cell_on_the_cell_picked() {
    struct Picker(Host);
    impl RunnerHooks for Picker {}
    impl GameHost for Picker {
        fn pick(&mut self, at: egui::Pos2) -> Option<(i32, i32)> {
            Some((at.x as i32 / 10, at.y as i32 / 10))
        }
    }
    let mut r = R::new(0, RouterSettings::default());
    let mut picker = Picker(Host::default());
    let env = Env { state: &r.st, scene: &r.scene, ctx: &r.ctx, camera: &mut r.cam, now: 0.0, watching: false, confirm_dangerous: false };
    let mut io = Io { runner: &mut r.runner, env, host: &mut picker };
    r.router.gesture(&Gesture::Click { pos: egui::pos2(61.0, 52.0), button: 1 }, &mut io);
    assert_eq!(r.runner.take_outbox(), vec![cm::click_cell(6, 5, 1)]);
    let _ = &picker.0;
}

#[test]
fn a_tap_that_starts_on_the_ui_never_reaches_the_view() {
    let mut g = Gestures::default();
    g.view = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(412.0, 915.0));
    // the dock's Autofight button
    g.ui = vec![egui::Rect::from_min_size(egui::pos2(340.0, 700.0), egui::vec2(60.0, 60.0))];
    let mut out = g.feed(&[touch(1, egui::TouchPhase::Start, 370.0, 730.0)], 0.0);
    out.extend(g.feed(&[touch(1, egui::TouchPhase::Move, 372.0, 731.0)], 50.0));
    out.extend(g.feed(&[touch(1, egui::TouchPhase::End, 372.0, 731.0)], 100.0));
    g.tick(800.0);
    out.extend(g.take());
    let k = kinds(&out);
    assert!(k.iter().all(|k| k != "click1" && !k.starts_with("hover") && k != "long"), "{k:?}");
    // even when the button has gone by the release (the dock hid)
    g.ui.clear();
    let mut out = g.feed(&[touch(2, egui::TouchPhase::Start, 370.0, 730.0)], 1000.0);
    g.ui = vec![egui::Rect::from_min_size(egui::pos2(340.0, 700.0), egui::vec2(60.0, 60.0))];
    out.extend(g.feed(&[touch(2, egui::TouchPhase::End, 370.0, 730.0)], 1100.0));
    assert!(kinds(&out).contains(&"click1".to_string()), "a press on the view is the view's");
}
