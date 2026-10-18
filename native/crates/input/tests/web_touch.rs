//! A finger in a browser: eframe's web runner hands a tap on as a `pointerdown` (a mouse
//! button's press, in a frame of its own), then the `touchstart` (another press and the Touch
//! Start), the `pointerup` (a release), and the `touchend` (a release, PointerGone and the Touch
//! End), in that order, as headless Chromium's touch emulation and Chrome on Android do.

use isorun_input::touch::*;

fn press(x: f32, y: f32, pressed: bool) -> egui::Event {
    egui::Event::PointerButton { pos: egui::pos2(x, y), button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
}

fn touch(phase: egui::TouchPhase, x: f32, y: f32) -> egui::Event {
    egui::Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(0), phase, pos: egui::pos2(x, y), force: Some(1.0) }
}

fn clicks(g: &[Gesture]) -> usize {
    g.iter().filter(|g| matches!(g, Gesture::Click { button: 1, .. })).count()
}

#[test]
fn a_browsers_tap_on_the_view_is_one_click() {
    let mut g = Gestures::default();
    let mut out = g.feed(&[press(120.0, 450.0, true)], 0.0);
    out.extend(g.feed(&[press(120.0, 450.0, true), touch(egui::TouchPhase::Start, 120.0, 450.0), press(120.0, 450.0, false)], 16.0));
    out.extend(g.feed(&[press(120.0, 450.0, false), egui::Event::PointerGone, touch(egui::TouchPhase::End, 120.0, 450.0)], 33.0));
    assert_eq!(clicks(&out), 1, "{out:?}");
}

#[test]
fn a_browsers_tap_whose_finger_drifts_a_little_is_still_a_click() {
    let mut g = Gestures::default();
    let mut out = g.feed(&[press(120.0, 450.0, true)], 0.0);
    out.extend(g.feed(&[press(120.0, 450.0, true), touch(egui::TouchPhase::Start, 120.0, 450.0), egui::Event::PointerMoved(egui::pos2(122.0, 452.0)), touch(egui::TouchPhase::Move, 122.0, 452.0)], 16.0));
    out.extend(g.feed(&[press(122.0, 452.0, false)], 60.0));
    out.extend(g.feed(&[press(122.0, 452.0, false), egui::Event::PointerGone, touch(egui::TouchPhase::End, 122.0, 452.0)], 80.0));
    assert_eq!(clicks(&out), 1, "{out:?}");
}

#[test]
fn a_mouse_click_is_still_a_click() {
    let mut g = Gestures::default();
    let mut out = g.feed(&[egui::Event::PointerMoved(egui::pos2(280.0, 470.0)), press(280.0, 470.0, true)], 0.0);
    out.extend(g.feed(&[press(280.0, 470.0, false)], 90.0));
    assert_eq!(clicks(&out), 1, "{out:?}");
}
