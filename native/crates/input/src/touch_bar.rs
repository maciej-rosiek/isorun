//! The touch bar (the port of `touch-bar.ts` and touch.css `.touch-bar`): on a touch screen, the
//! pad's bindings of the moment as buttons along the foot of the screen, in every mode but
//! command mode and the others whose screen has its own (BARE_MODES).

use crate::bindings::{bar_labels, Action, BindingLabel, NO_ACTION};
use crate::context::{Context, Mode};
use crate::gamepad::Button;
use isorun_webtiles::formatted_string_to_text;

const TOUCHED_BUTTONS: [Button; 8] = [Button::B, Button::LB, Button::LT, Button::X, Button::Y, Button::RT, Button::RB, Button::A];
const BARE_MODES: [Mode; 9] = [Mode::Command, Mode::Macro, Mode::Text, Mode::Yesno, Mode::Prompt, Mode::Lobby, Mode::Spectating, Mode::Newgame, Mode::Dialog];

/// The bar's buttons: the bindings of the touched buttons that do something here, each label
/// once, A last (at the right, under the thumb), B first.
pub fn touch_labels(ctx: &Context) -> Vec<BindingLabel> {
    let order = |b: Button| TOUCHED_BUTTONS.iter().position(|x| *x == b).unwrap_or(usize::MAX);
    let mut labels: Vec<BindingLabel> = bar_labels(ctx).into_iter().filter(|l| TOUCHED_BUTTONS.contains(&l.button) && !l.teaching && !l.label.is_empty() && l.label != NO_ACTION).collect();
    // sorted from A back to B, so the first of a label kept is the one nearest A
    labels.sort_by(|a, b| order(b.button).cmp(&order(a.button)));
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for l in labels {
        if seen.contains(&l.label) {
            continue;
        }
        seen.push(l.label.clone());
        out.push(l);
    }
    out.reverse();
    out
}

/// The labels the bar shows now: none where it is hidden.
pub fn shown_labels(ctx: &Context, shown: bool) -> Vec<BindingLabel> {
    if shown && !BARE_MODES.contains(&ctx.mode) {
        touch_labels(ctx)
    } else {
        Vec::new()
    }
}

const BRONZE: egui::Color32 = egui::Color32::from_rgb(0x9c, 0x7a, 0x45);
const BRONZE_DIM: egui::Color32 = egui::Color32::from_rgb(0x4d, 0x3d, 0x24);
const BRONZE_BRIGHT: egui::Color32 = egui::Color32::from_rgb(0xe8, 0xc9, 0x8a);
const INK: egui::Color32 = egui::Color32::from_rgb(0xe9, 0xe2, 0xd0);
const INK_DIM: egui::Color32 = egui::Color32::from_rgb(0x9d, 0x94, 0x83);

/// The bar, drawn along the foot of `screen` (the safe area's rectangle): returns the action of
/// the button tapped this frame, and the bar's rectangle (None when it is hidden).
pub fn touch_bar(ui: &mut egui::Ui, ctx: &Context, shown: bool, screen: egui::Rect) -> (Option<Action>, Option<egui::Rect>) {
    let labels = shown_labels(ctx, shown);
    if labels.is_empty() {
        return (None, None);
    }
    let caps = crate::cell_menu::caps_family(ui.ctx());
    let font = egui::FontId::new(13.0, caps);
    let (gap, pad, h) = (6.0, 8.0, 42.0);
    // the buttons' widths: the label plus 14px each side, at most 14em
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| {
            let text = formatted_string_to_text(&l.label);
            let w = ui.fonts_mut(|f| f.layout_no_wrap(text, font.clone(), INK).size().x);
            (w + 28.0).min(14.0 * 13.0)
        })
        .collect();
    // flex-wrap: wrap-reverse, centred: rows fill from the bottom up
    let avail = screen.width() - 2.0 * pad;
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut row_w = 0.0;
    for (i, w) in widths.iter().enumerate() {
        let need = if rows.last().unwrap().is_empty() { *w } else { row_w + gap + w };
        if need > avail && !rows.last().unwrap().is_empty() {
            rows.push(vec![i]);
            row_w = *w;
        } else {
            rows.last_mut().unwrap().push(i);
            row_w = need;
        }
    }
    let bar_h = rows.len() as f32 * h + (rows.len() as f32 - 1.0) * gap + 2.0 * pad;
    let bar = egui::Rect::from_min_max(egui::pos2(screen.left(), screen.bottom() - bar_h), screen.max);
    let painter = ui.painter().clone().with_layer_id(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("isorun-touch-bar")));
    // linear-gradient(180deg, rgba(10,9,8,0.75), rgba(10,9,8,0.97) 30%), border-top bronze-dim
    let split = egui::Rect::from_min_max(bar.min, egui::pos2(bar.right(), bar.top() + bar.height() * 0.3));
    crate::cell_menu::vertical_gradient(&painter, split, egui::Color32::from_rgba_unmultiplied(10, 9, 8, 191), egui::Color32::from_rgba_unmultiplied(10, 9, 8, 247));
    painter.rect_filled(egui::Rect::from_min_max(split.left_bottom(), bar.max), 0.0, egui::Color32::from_rgba_unmultiplied(10, 9, 8, 247));
    painter.hline(bar.x_range(), bar.top(), egui::Stroke::new(1.0, BRONZE_DIM));
    let mut tapped = None;
    // the bar is a layer of its own over the dock: its buttons take the taps, and the rest of
    // the bar keeps them from the dock's buttons under it (a tap on one of the bar's went to the
    // quick slot under it)
    let ectx = ui.ctx().clone();
    egui::Area::new(egui::Id::unique("isorun-touch-bar-area")).order(egui::Order::Foreground).fixed_pos(bar.min).show(&ectx, |ui| {
    ui.interact(bar, egui::Id::unique("isorun-touch-bar-block"), egui::Sense::click());
    for (r, row) in rows.iter().enumerate() {
        // wrap-reverse: the first row is the bottom one
        let y = bar.bottom() - pad - h - r as f32 * (h + gap);
        let total: f32 = row.iter().map(|&i| widths[i]).sum::<f32>() + gap * (row.len() as f32 - 1.0);
        let mut x = bar.center().x - total / 2.0;
        for &i in row {
            let l = &labels[i];
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(widths[i], h));
            x += widths[i] + gap;
            let resp = ui.interact(rect, egui::Id::unique(("isorun-touch-button", l.button)), egui::Sense::click());
            let (top, bottom, border, ink) = match l.button {
                Button::A => (egui::Color32::from_rgba_unmultiplied(74, 58, 30, 245), egui::Color32::from_rgba_unmultiplied(24, 18, 10, 245), BRONZE, BRONZE_BRIGHT),
                Button::B => (egui::Color32::from_rgba_unmultiplied(42, 36, 29, 242), egui::Color32::from_rgba_unmultiplied(14, 12, 9, 242), BRONZE_DIM, INK_DIM),
                _ => (egui::Color32::from_rgba_unmultiplied(42, 36, 29, 242), egui::Color32::from_rgba_unmultiplied(14, 12, 9, 242), BRONZE_DIM, INK),
            };
            painter.add(egui::epaint::Shadow { offset: [0, 3], blur: 10, spread: 0, color: egui::Color32::from_black_alpha(153) }.as_shape(rect, 6.0));
            crate::cell_menu::vertical_gradient(&painter, rect.shrink(1.0), top, bottom);
            painter.rect_stroke(rect, 6.0, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
            let text = formatted_string_to_text(&l.label);
            let galley = painter.layout(text, font.clone(), ink, (widths[i] - 28.0).max(1.0));
            painter.galley(rect.center() - galley.size() / 2.0, galley, ink);
            if resp.clicked() {
                tapped = Some(l.action.clone());
            }
        }
    }
    });
    (tapped, Some(bar))
}
