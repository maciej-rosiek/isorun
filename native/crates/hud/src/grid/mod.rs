//! The console grid the game screen is laid out on (grid/console.ts, grid/host.ts), the text rows
//! its panes print (grid/rows.ts), and the classic HUD's stats and message panes as rows
//! (grid/stats.ts, grid/messages.ts), the action panel's arithmetic (grid/panel.ts), and the
//! painting of a row (grid/paint.ts).

pub mod console;
pub mod messages;
pub mod panel;
pub mod rows;
pub mod stats;

use crate::paint::{rgba, C};
use crate::style::{class_colour, term};
use egui::{Color32, Painter, Pos2, Rect};
use rows::{BarPaint, Bg, Row};

/// The stats bars' track (`--stats-bar-track`): the palette's darkgray 45% over black.
pub fn bar_track() -> C {
    term(8).mix(rgba(0, 0, 0, 1.0), 0.45)
}

/// How a row's text is shadowed over the view: the panes are bare, their text shadowed instead.
pub const PANE_SHADOW: [(f32, f32, f32, Color32); 2] = crate::paint::VIEW_TEXT_SHADOW;

/// One row of grid text painted at `at`, one cell (`cw` × `ch`) a character: each span's
/// background over its cells, then the text in the grid's font, `fg` (a terminal colour) or the
/// class's colour or the pane's `default` colour, shadowed. Returns the spans' rects with their
/// tooltips, for the pane to show on hover.
pub fn paint_row(painter: &Painter, row: &Row, at: Pos2, cw: f32, ch: f32, font: f32, default: C, shadow: bool) -> Vec<(Rect, String)> {
    let mut x = at.x;
    let mut titled = vec![];
    let font_id = egui::FontId::monospace(font);
    for s in row {
        let n = s.text.chars().count() as f32;
        let rect = Rect::from_min_size(egui::pos2(x, at.y), egui::vec2(n * cw, ch));
        match &s.bg {
            Some(Bg::Term(i)) => {
                painter.rect_filled(rect, 0.0, term(*i as i64).to_color32());
            }
            Some(Bg::Rgb([r, g, b])) => {
                painter.rect_filled(rect, 0.0, Color32::from_rgb(*r, *g, *b));
            }
            Some(Bg::Stripes(stops)) => {
                for (a, b, paint) in stops {
                    let c = match paint {
                        BarPaint::Term(i) => term(*i as i64),
                        BarPaint::Track => bar_track(),
                    };
                    let r =
                        Rect::from_min_max(egui::pos2(rect.left() + rect.width() * a, rect.top()), egui::pos2(rect.left() + rect.width() * b, rect.bottom()));
                    painter.rect_filled(r, 0.0, c.to_color32());
                }
            }
            None => {}
        }
        let colour = s.fg.map(|f| term(f as i64)).or_else(|| s.cls.as_deref().and_then(class_colour)).unwrap_or(default);
        if !s.text.trim().is_empty() {
            // each character on its own cell, as the grid's font has the cell's advance
            let g = painter.ctx().fonts_mut(|f| f.layout_no_wrap(s.text.clone(), font_id.clone(), Color32::WHITE));
            let y = at.y + (ch - g.size().y) / 2.0;
            if shadow {
                crate::paint::shadowed_galley(painter, egui::pos2(x, y), g, colour.to_color32(), &PANE_SHADOW);
            } else {
                painter.galley_with_override_text_color(egui::pos2(x, y), g, colour.to_color32());
            }
        }
        if let Some(t) = &s.title {
            titled.push((rect, t.clone()));
        }
        x += n * cw;
    }
    titled
}
