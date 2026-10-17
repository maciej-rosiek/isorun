//! A CRT text screen (overlays.ts `crtBody`): one line per row of the server's own HTML
//! (tileweb-text.cc WebTextArea), and on a screen the scraper knows (the skills screen) a cursor
//! marker over each row it prints a hotkey for, measured in `ch` since the screen is monospace.

use crate::crt_scrape::{scrape_crt, CrtKind};
use crate::focus::{Act, FocusOptions, Focusable};
use crate::frame::{self, Screen};
use crate::menu::paint_focused;
use crate::rich::{self, TextStyle};
use crate::{OverlayCx, Overlays};
use egui::{Pos2, Rect, Vec2};
use isorun_ui::widgets::{self, hex, rgba};
use isorun_webtiles::{cm, GameState};

/// The screen's lines: the crt area, then every other area.
pub fn lines(state: &GameState) -> Vec<String> {
    let mut out = Vec::new();
    let mut areas: Vec<(&String, &std::collections::BTreeMap<i64, String>)> = state.crt.areas.iter().filter(|(k, _)| k.as_str() != "crt").collect();
    areas.sort_by(|a, b| a.0.cmp(b.0));
    for area in std::iter::once(&state.crt.lines).chain(areas.into_iter().map(|a| a.1)) {
        let max = area.keys().max().copied().unwrap_or(-1);
        for i in 0..=max {
            out.push(area.get(&i).cloned().unwrap_or_default());
        }
    }
    out
}

/// The cursor's stops on the screen. skill-menu.cc: `=` puts the screen in set-target mode and a
/// skill's letter then asks for its target, so a row's second action (Y) sends both.
pub fn focus(state: &GameState, tag: &str) -> (String, Vec<Focusable>, FocusOptions) {
    let hot = scrape_crt(tag, &lines(state));
    let targets = tag == "skills" && hot.iter().any(|h| h.kind == CrtKind::Footer && (h.key == "=" || h.label.contains("skill target") || h.label.contains("selected target")));
    let items = hot
        .iter()
        .map(|h| {
            // rows sit in the screen's columns; footer switches get columns of their own
            let n: i32 = h.group.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
            let col = if h.kind == CrtKind::Row { n } else { 10 + n };
            let kind = if h.kind == CrtKind::Row { "row" } else { "footer" };
            let mut f = Focusable { label: h.label.clone(), act: Act::send([cm::input(&h.key)]), row: Some(h.line as i32), col: Some(col), group: h.group.clone(), id: Some(format!("{kind}:{}", h.key)), ..Default::default() };
            if targets && h.kind == CrtKind::Row {
                f.alt = Some(("Set target".into(), Act::send([cm::input(&format!("={}", h.key))])));
            }
            f
        })
        .collect();
    // two columns of rows, a row often with an entry in only one: left / right cross columns
    (format!("crt:{tag}"), items, FocusOptions { wrap: true, page_rows: 8, columns: true, ..Default::default() })
}

impl Overlays {
    pub(crate) fn draw_crt(&mut self, ctx: &egui::Context, cx: &mut OverlayCx, tag: &str, top: bool, depth: usize, screen: &Screen, stack: i64) {
        let font = screen.popup_font();
        // `.crt .crt-line { line-height: 1.2em }`
        let st = TextStyle::new(ctx, font, hex(0xdddddd)).line(1.2 * font);
        let pad_y = 0.5 * screen.rem();
        let pad_x = 0.8 * screen.rem();
        let ls = lines(cx.state);
        let galleys: Vec<_> = ls.iter().map(|l| rich::galley(ctx, rich::html(if l.is_empty() { " " } else { l }, &st, 0.0))).collect();
        let natural = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max);
        let width = (natural + 2.0 * pad_x + 2.0).ceil().clamp(screen.popup_min_w(), screen.popup_max_w());
        let line_h = 1.2 * font;
        let content_h = ls.len() as f32 * line_h + 2.0 * pad_y;
        let height = content_h.min(screen.popup_max_h()) + 2.0;
        let rect = frame::place(screen, Vec2::new(width, height), depth, false, false);
        let hot = if top { scrape_crt(tag, &ls) } else { Vec::new() };
        let cw = widgets::ch(font);
        let lit = if top { self.nav.current().map(|f| f.key().to_string()) } else { None };
        let dock = cx.dock;
        let mut clicked = None;
        let key = stack;
        let scroll = self.scroll.get(&key).copied().unwrap_or(0.0);
        let mut new_scroll = scroll;
        frame::layer(ctx, egui::Id::unique(("isorun-crt", stack)), rect, |ui, _| {
            let p = ui.painter().clone();
            widgets::paint_panel(&p, rect, &frame::look(dock));
            let view = rect.shrink(1.0);
            new_scroll = frame::wheel(ui, view, content_h, scroll);
            let bp = p.with_clip_rect(view);
            let left = rect.left() + 1.0 + pad_x;
            let top_y = rect.top() + 1.0 + pad_y - new_scroll;
            for h in &hot {
                let r = Rect::from_min_size(Pos2::new(left + h.col as f32 * cw, top_y + h.line as f32 * line_h), Vec2::new(h.len as f32 * cw, line_h));
                let kind = if h.kind == CrtKind::Row { "row" } else { "footer" };
                let id = format!("{kind}:{}", h.key);
                let resp = ui.interact(r, egui::Id::unique(("isorun-crt-hot", &id)), egui::Sense::click());
                if lit.as_deref() == Some(id.as_str()) {
                    paint_focused(&bp, r);
                } else if resp.hovered() {
                    widgets::outline(&bp, r, 3.0, rgba(255, 212, 92, 0.35));
                }
                if resp.clicked() {
                    clicked = Some(h.key.clone());
                }
            }
            for (i, g) in galleys.iter().enumerate() {
                bp.galley(Pos2::new(left, top_y + i as f32 * line_h), g.clone(), st.colour);
            }
        });
        self.scroll.insert(key, new_scroll);
        if let Some(k) = clicked {
            cx.send(cm::input(&k));
        }
    }
}
