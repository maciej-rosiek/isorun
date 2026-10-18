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

/// The scroll bar every scrolling screen shows, one look for all: a thin gold thumb on a faint
/// track at the right edge of `viewport`, wherever the body is taller than it, so a player sees
/// there is more and where they are in it. It is always there while the body scrolls (a phone has
/// no hover to call up egui's own floating bars, which is why those are hidden), and takes no
/// room from the body (the web build's bars overlay the page). `hot` is a mouse over it or
/// dragging it: brighter.
pub fn paint_bar_lit(painter: &egui::Painter, viewport: Rect, content_h: f32, scroll: f32, hot: bool) {
    let Some((track, thumb)) = bar_rects(viewport, content_h, scroll) else { return };
    painter.rect_filled(track, 2.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, if hot { 34 } else { 22 }));
    painter.rect_filled(thumb, 2.0, egui::Color32::from_rgba_unmultiplied(216, 180, 95, if hot { 235 } else { 185 }));
}

/// [`paint_bar_lit`], not hot: for a body that only shows where it is.
pub fn paint_bar(painter: &egui::Painter, viewport: Rect, content_h: f32, scroll: f32) {
    paint_bar_lit(painter, viewport, content_h, scroll, false);
}

/// The bar's track and thumb, or None when the body fits.
fn bar_rects(viewport: Rect, content_h: f32, scroll: f32) -> Option<(Rect, Rect)> {
    let max = content_h - viewport.height();
    if max <= 1.0 || viewport.height() < 12.0 {
        return None;
    }
    let track = Rect::from_min_max(egui::pos2(viewport.right() - 6.0, viewport.top() + 4.0), egui::pos2(viewport.right() - 2.0, viewport.bottom() - 4.0));
    let h = (track.height() * viewport.height() / content_h).clamp(18.0_f32.min(track.height()), track.height());
    let t = (scroll / max).clamp(0.0, 1.0);
    let thumb = Rect::from_min_size(egui::pos2(track.left(), track.top() + t * (track.height() - h)), egui::vec2(track.width(), h));
    Some((track, thumb))
}

/// The bar, which a mouse can also drag (and a click on its track pages): returns the scroll
/// after it. The strip it answers on is a little wider than it is drawn and takes only drags, so
/// a click or a tap at the edge still reaches what is under it.
pub fn bar(ui: &egui::Ui, id: egui::Id, viewport: Rect, content_h: f32, scroll: f32) -> f32 {
    let max = (content_h - viewport.height()).max(0.0);
    let Some((track, thumb)) = bar_rects(viewport, content_h, scroll) else { return scroll.clamp(0.0, max) };
    let hit = Rect::from_min_max(egui::pos2(viewport.right() - 10.0, track.top()), egui::pos2(viewport.right(), track.bottom()));
    let resp = ui.interact(hit, id, egui::Sense::drag());
    let mut s = scroll;
    if resp.dragged() {
        // the thumb follows the pointer: a point of the track is this much of the body
        s += resp.drag_delta().y * (max / (track.height() - thumb.height()).max(1.0));
    }
    let s = s.clamp(0.0, max);
    paint_bar_lit(ui.painter(), viewport, content_h, s, resp.hovered() || resp.dragged());
    s
}

/// An egui [`egui::ScrollArea`] in the same look: its own bars hidden (floating ones show only
/// under a hovering mouse, never on a phone), [`bar`] drawn over its right edge instead. `key`
/// names it for the bar's drag, which reaches the area on the next frame.
pub fn area<R>(ui: &mut egui::Ui, key: egui::Id, area: egui::ScrollArea, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::scroll_area::ScrollAreaOutput<R> {
    let mut area = area.scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(off) = ui.ctx().data_mut(|d| d.remove_temp::<f32>(key)) {
        area = area.vertical_scroll_offset(off);
    }
    let out = area.show(ui, add);
    let off = out.state.clamped_offset().y;
    let s = bar(ui, key.with("bar"), out.inner_rect, out.content_size.y, off);
    if (s - off).abs() > 0.1 {
        ui.ctx().data_mut(|d| d.insert_temp(key, s));
        ui.ctx().request_repaint();
    }
    out
}
