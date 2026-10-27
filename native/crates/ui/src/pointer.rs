//! Where a press landed this frame, read off the frame's events. A quick tap on a touch screen
//! (and any tap in a browser, whose frames come slower than a finger) can bring its press, its
//! release and the PointerGone that ends a touch in one frame; egui's release clears
//! `press_origin`, and PointerGone `latest_pos`, so in that frame the pointer state says nothing
//! of where the press was. The events still do.

use egui::{Event, PointerButton, Pos2};

/// The first press of `button` this frame, where it landed.
pub fn pressed_at(ctx: &egui::Context, button: PointerButton) -> Option<Pos2> {
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            Event::PointerButton { pos, button: b, pressed: true, .. } if *b == button => Some(*pos),
            _ => None,
        })
    })
}

/// The first press of any button this frame: where, and which.
pub fn any_pressed_at(ctx: &egui::Context) -> Option<(Pos2, PointerButton)> {
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            Event::PointerButton { pos, button, pressed: true, .. } => Some((*pos, *button)),
            _ => None,
        })
    })
}

/// How long a finger rests on something before its card shows (and a press that long is not a
/// tap): a long press, as a phone's own.
pub const LONG_PRESS: f64 = 0.35;

#[derive(Clone, Copy, Default)]
struct Touchy {
    /// a coarse pointer: the app runs for touch (`--touch`, a phone)
    coarse: bool,
    /// the last pointer input was a finger's
    finger: bool,
    /// when the primary button (or finger) last went down, in egui's time
    pressed_at: Option<f64>,
    /// how long the last press that ended was held
    last_hold: f64,
}

fn touchy_id() -> egui::Id {
    egui::Id::NULL.with("isorun-pointer-touchy")
}

fn touchy(ctx: &egui::Context) -> Touchy {
    ctx.data(|d| d.get_temp::<Touchy>(touchy_id())).unwrap_or_default()
}

/// Read this frame's pointer events (once a frame, before the screens draw): whether a finger or
/// a mouse spoke last, and how long the press that ended was held. `coarse`: the app runs for
/// touch, where a mouse's hover is not to be trusted either.
pub fn note_frame(ctx: &egui::Context, coarse: bool) {
    let mut t = touchy(ctx);
    t.coarse = coarse;
    ctx.input(|i| {
        let touched = i.events.iter().any(|e| matches!(e, Event::Touch { .. }));
        if touched {
            t.finger = true;
        } else if i.events.iter().any(|e| matches!(e, Event::PointerMoved(_) | Event::PointerButton { .. })) {
            // a mouse moved or clicked, with no finger in the frame
            t.finger = false;
        }
        for e in &i.events {
            if let Event::PointerButton { button: PointerButton::Primary, pressed, .. } = e {
                if *pressed {
                    t.pressed_at = Some(i.time);
                } else if let Some(at) = t.pressed_at.take() {
                    t.last_hold = i.time - at;
                }
            }
        }
    });
    ctx.data_mut(|d| d.insert_temp(touchy_id(), t));
}

/// Whether hover means nothing here: a finger spoke last, or the app runs for touch.
pub fn touch_mode(ctx: &egui::Context) -> bool {
    let t = touchy(ctx);
    t.coarse || t.finger
}

/// Whether a hover card over `over` may show. With a mouse, whenever the pointer is over it (the
/// caller's hover). On touch only while a finger rests on it, held there a long press: a finger's
/// tap leaves egui a pointer position it keeps as a hover (a browser may never say the pointer has
/// gone), and a card from that stood over the inventory after every tap.
pub fn card_shows(ctx: &egui::Context, hovered: bool, over: egui::Rect) -> bool {
    if !touch_mode(ctx) {
        return hovered;
    }
    let t = touchy(ctx);
    let (down, origin, at, now) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.press_origin(), i.pointer.latest_pos(), i.time));
    let Some(pressed) = t.pressed_at.filter(|_| down) else { return false };
    if !origin.is_some_and(|p| over.contains(p)) || !at.is_some_and(|p| over.contains(p)) {
        return false;
    }
    let held = now - pressed;
    if held < LONG_PRESS {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(LONG_PRESS - held));
        return false;
    }
    true
}

/// Whether a click is a tap that acts: on touch, a press held as long as a long press showed a
/// card and is not also a tap, nor is the click egui makes of a finger still held past its click
/// time (its "press and hold for a context menu").
pub fn tap(ctx: &egui::Context, clicked: bool) -> bool {
    if !clicked || !touch_mode(ctx) {
        return clicked;
    }
    let still_down = ctx.input(|i| i.pointer.primary_down());
    !still_down && touchy(ctx).last_hold < LONG_PRESS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame at `time` with `events`; returns whether a card over `over` shows in it.
    fn frame(ctx: &egui::Context, time: f64, events: Vec<Event>, over: egui::Rect) -> bool {
        let mut shows = false;
        let raw = egui::RawInput {
            time: Some(time),
            events,
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 800.0))),
            ..Default::default()
        };
        let mut out = ctx.run_pass(raw, |ctx| {
            note_frame(ctx, false);
            shows = card_shows(ctx, true, over);
        });
        out.textures_delta.clear();
        shows
    }

    fn touch(phase: egui::TouchPhase, p: Pos2) -> Event {
        Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(0), phase, pos: p, force: None }
    }

    fn button(p: Pos2, pressed: bool) -> Event {
        Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
    }

    #[test]
    fn a_finger_shows_a_card_only_while_it_rests_and_a_mouse_whenever_it_hovers() {
        let ctx = egui::Context::default();
        let over = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(50.0, 50.0));
        let p = egui::pos2(30.0, 30.0);
        // a tap: no card, during it or after it, though egui keeps its place as a hover
        assert!(!frame(&ctx, 0.0, vec![touch(egui::TouchPhase::Start, p), Event::PointerMoved(p), button(p, true)], over));
        assert!(!frame(&ctx, 0.1, vec![touch(egui::TouchPhase::End, p), button(p, false)], over));
        assert!(!frame(&ctx, 1.0, vec![], over));
        // a finger held: the card after a long press, gone when it lifts
        assert!(!frame(&ctx, 2.0, vec![touch(egui::TouchPhase::Start, p), Event::PointerMoved(p), button(p, true)], over));
        assert!(frame(&ctx, 2.5, vec![], over));
        assert!(!frame(&ctx, 2.6, vec![touch(egui::TouchPhase::End, p), button(p, false), Event::PointerGone], over));
        // a mouse moving over it: the hover's
        assert!(frame(&ctx, 3.0, vec![Event::PointerMoved(p)], over));
    }
}
