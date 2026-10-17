//! Scrolling a hand-laid-out body (a screen painted at its own positions, not in egui's
//! `ScrollArea`): the wheel over it, and a finger or the mouse dragging it, which is how a phone
//! scrolls. A drag only scrolls once it is decidedly one, so a tap still clicks what it is on.

use egui::Rect;

/// The new scroll of a body `content_h` tall shown in `viewport`, kept in range.
pub fn scroll(ui: &egui::Ui, viewport: Rect, content_h: f32, scroll: f32) -> f32 {
    let max = (content_h - viewport.height()).max(0.0);
    let (hover, wheel, drag) = ui.input(|i| {
        let drag = (i.pointer.primary_down() && i.pointer.is_decidedly_dragging()).then(|| (i.pointer.press_origin(), i.pointer.delta().y));
        (i.pointer.hover_pos(), i.smooth_scroll_delta.y, drag)
    });
    let mut s = scroll;
    if hover.is_some_and(|p| viewport.contains(p)) {
        s -= wheel;
    }
    if let Some((Some(origin), dy)) = drag {
        if viewport.contains(origin) {
            s -= dy;
        }
    }
    s.clamp(0.0, max)
}

/// Whether the pointer is dragging a body that started in `viewport` (a tap that became a scroll
/// clicks nothing).
pub fn dragging(ui: &egui::Ui, viewport: Rect) -> bool {
    ui.input(|i| i.pointer.is_decidedly_dragging() && i.pointer.press_origin().is_some_and(|p| viewport.contains(p)))
}

/// A thin bar at the right edge of `viewport` where the body is taller than it: how much there is
/// and where the view is, so a phone's player sees there is more to scroll to.
pub fn paint_bar(painter: &egui::Painter, viewport: Rect, content_h: f32, scroll: f32) {
    if content_h <= viewport.height() + 0.5 {
        return;
    }
    let track = Rect::from_min_max(egui::pos2(viewport.right() - 5.0, viewport.top() + 4.0), egui::pos2(viewport.right() - 2.0, viewport.bottom() - 4.0));
    let h = (track.height() * viewport.height() / content_h).max(16.0);
    let t = scroll / (content_h - viewport.height());
    let thumb = Rect::from_min_size(egui::pos2(track.left(), track.top() + t * (track.height() - h)), egui::vec2(track.width(), h));
    painter.rect_filled(track, 1.5, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14));
    painter.rect_filled(thumb, 1.5, egui::Color32::from_rgba_unmultiplied(216, 180, 95, 150));
}
