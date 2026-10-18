//! Where a press landed this frame, read off the frame's events. A quick tap on a touch screen
//! (and any tap in a browser, whose frames come slower than a finger) can bring its press, its
//! release and the PointerGone that ends a touch in one frame; egui's release clears
//! `press_origin`, and PointerGone `latest_pos`, so in that frame the pointer state says nothing
//! of where the press was. The events still do.

use egui::{Event, PointerButton, Pos2};

/// The first press of `button` this frame, where it landed.
pub fn pressed_at(ctx: &egui::Context, button: PointerButton) -> Option<Pos2> {
    ctx.input(|i| i.events.iter().find_map(|e| match e {
        Event::PointerButton { pos, button: b, pressed: true, .. } if *b == button => Some(*pos),
        _ => None,
    }))
}

/// The first press of any button this frame: where, and which.
pub fn any_pressed_at(ctx: &egui::Context) -> Option<(Pos2, PointerButton)> {
    ctx.input(|i| i.events.iter().find_map(|e| match e {
        Event::PointerButton { pos, button, pressed: true, .. } => Some((*pos, *button)),
        _ => None,
    }))
}
