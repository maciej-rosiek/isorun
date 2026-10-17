//! Drawing a screen of the front end (`menu.ts list()` and the `styles.css` rules it stands on):
//! the room behind, its veil and the shade over it, then the frame in the safe area: the head
//! (the wordmark on the title screen), the notices, the rows with what stands beside them, the
//! region under them (a document, the roster, the settings), the message line and the legend.

use crate::front::*;
use crate::paint::*;
use crate::settings;
use crate::words::{self, NavDir};
use egui::{pos2, vec2, Align2, Color32, Rect, Sense};
use std::sync::Arc;

/// The sizes a screen is laid out with, from the window and the UI scale (the CSS's rem and media
/// queries).
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub rem: f32,
    pub w: f32,
    pub h: f32,
    /// `max-width: 600px`: a phone.
    pub phone: bool,
    /// `(max-width: 600px), (max-height: 700px)`: less air.
    pub compact: bool,
    /// `max-height: 600px`: a phone on its side, a handheld: the head on one line.
    pub short: bool,
    /// `max-width: 700px`: the heads cap their size.
    pub narrow: bool,
    pub faces: Faces,
    /// The room's grid cell font size (`--fs`, room/view.ts `fit`).
    pub fs: f32,
}

impl Metrics {
    pub fn new(rect: Rect, ui_scale: f32, ppp: f32) -> Self {
        let (w, h) = (rect.width(), rect.height());
        let cw = ((w / 59.0).max(h / (32.0 * 1.45))).ceil().clamp(11.0, 18.0);
        Metrics {
            rem: 15.0 * ui_scale,
            w,
            h,
            phone: w <= 600.0,
            compact: w <= 600.0 || h <= 700.0,
            short: h <= 600.0,
            narrow: w <= 700.0,
            faces: Faces { bold_text: ppp <= 1.5 },
            fs: (cw * 1.55).round(),
        }
    }

    /// The frame's padding: the console's title-safe area (6vw, 7vh), less on a small window.
    pub fn pad(&self) -> (f32, f32, f32) {
        if self.compact {
            let y = if self.short { 0.6 * self.rem } else { 1.25 * self.rem };
            (1.25 * self.rem, y, y)
        } else {
            (0.06 * self.w, 0.07 * self.h, 0.07 * self.h)
        }
    }

    pub fn action(&self) -> f32 {
        1.35 * self.rem
    }

    pub fn note(&self) -> f32 {
        1.1 * self.rem
    }

    pub fn row_height(&self) -> f32 {
        3.4 * self.rem
    }

    pub fn label_inset(&self) -> f32 {
        2.5 * self.rem
    }

    pub fn cursor_inset(&self) -> f32 {
        0.9 * self.rem
    }

    pub fn text(&self, size: f32, colour: Color32) -> Style {
        Style::new(size, self.faces.text(), colour)
    }
}

/// What the mouse did to the frame this time.
#[derive(Default)]
pub(crate) struct Clicks {
    pub chosen: Option<(String, Act)>,
    pub hovered: Option<String>,
    /// A setting's arrow: its key and the way it turns.
    pub turn: Option<(String, i32)>,
}

impl FrontEnd {
    /// Draw `s` filling the window; the mouse and the keys are taken here too.
    pub(crate) fn draw(&mut self, ui: &mut egui::Ui, s: &Screen) {
        let rect = ui.max_rect();
        let ctx = ui.ctx().clone();
        let m = Metrics::new(rect, settings::ui_scale(&self.settings), ctx.pixels_per_point());
        self.settle_cursor(s);
        self.keys(ui, s);
        let s = &self.screen();
        self.settle_cursor(s);
        // the row under the cursor may read differently now (Play became Continue): the line follows it
        self.on_cursor(s);
        // the room, its veil, the shade the menus stand on
        let background = ui.painter().clone();
        background.rect_filled(rect, 0.0, Color32::from_rgb(0x03, 0x03, 0x05));
        radial(&background, rect, pos2(rect.left() + rect.width() * 0.28, rect.top() + rect.height() * 0.32), &[(0.0, Color32::from_rgb(0x17, 0x13, 0x1f)), (0.58, Color32::from_rgb(0x09, 0x08, 0x0d)), (1.0, Color32::from_rgb(0x03, 0x03, 0x05))]);
        let rs = self.rs.clone();
        let settings = self.settings.clone();
        self.room.paint(ui, &rs, rect, &settings);
        gradient(&background, rect, false, &crate::room::Tint::veil(self.room.place.tint));
        if s.cls == Cls::Home || self.view == View::Exit {
            gradient(&background, rect, true, &[(0.0, rgba(3, 2, 6, 0.9)), (0.4, rgba(3, 2, 6, 0.8)), (0.56, rgba(3, 2, 6, 0.45)), (0.74, rgba(3, 2, 6, 0.12)), (1.0, rgba(3, 2, 6, 0.06))]);
            gradient(&background, rect, false, &[(0.0, rgba(3, 2, 6, 0.45)), (0.25, rgba(3, 2, 6, 0.0)), (0.7, rgba(3, 2, 6, 0.0)), (1.0, rgba(3, 2, 6, 0.7))]);
            if m.phone {
                gradient(&background, rect, false, &[(0.0, rgba(3, 2, 6, 0.55)), (1.0, rgba(3, 2, 6, 0.75))]);
            }
        } else {
            radial(&background, rect, pos2(rect.center().x, rect.top() + rect.height() * 0.45), &[(0.0, rgba(5, 4, 9, 0.12)), (0.62, rgba(4, 3, 8, 0.45)), (1.0, rgba(2, 2, 4, 0.75))]);
        }
        // a drag on the scenery looks around the room; a press that never moves does nothing
        let scenery = ui.interact(rect, egui::Id::NULL.with("room-scenery"), Sense::drag());
        if scenery.dragged() {
            let total = scenery.drag_delta().x;
            if ui.input(|i| i.pointer.press_origin().zip(i.pointer.interact_pos()).is_some_and(|(a, b)| (a - b).length() >= crate::room::DRAG_SLOP)) {
                self.room.drag(total);
            }
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if scenery.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        if self.view == View::Boot {
            let g = galley(&ctx, &self.boot_text, &m.text(m.rem * 1.1, INK_DIM), f32::INFINITY);
            ui.painter().galley(rect.center() - g.size() / 2.0, g, INK_DIM);
            return;
        }
        let (px, top, bottom) = m.pad();
        let frame = Rect::from_min_max(pos2(rect.left() + px, rect.top() + top), pos2(rect.right() - px, rect.bottom() - bottom));
        // the frame slides in as a screen is entered
        let since = (isorun_client::platform::seconds() - self.arrived_at) as f32;
        let t = if self.room.reduced_motion { 1.0 } else { (since / 0.14).clamp(0.0, 1.0) };
        let ease = 1.0 - (1.0 - t).powi(3);
        if t < 1.0 {
            ctx.request_repaint();
        }
        let shift = vec2(0.0, 12.0 * (1.0 - ease));
        let mut clicks = Clicks::default();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(frame.translate(shift)).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_opacity(ease);
        child.spacing_mut().item_spacing = vec2(0.0, 0.0);
        if s.over {
            self.draw_dialog(&mut child, s, &m, frame, &mut clicks);
        } else if s.cls == Cls::Home {
            self.draw_home(&mut child, s, &m, &mut clicks);
        } else {
            self.draw_list(&mut child, s, &m, &mut clicks);
        }
        if s.cls == Cls::Home {
            // the release, in the screen's very corner: the smallest, dimmest words on the screen
            let g = galley(&ctx, crate::news::VERSION, &Style::new(0.7 * m.rem, m.faces.text(), INK_DIM), f32::INFINITY);
            let at = pos2(rect.right() - (0.6 * m.rem).max(0.0) - g.size().x, rect.bottom() - 0.5 * m.rem - g.size().y);
            ui.painter().galley_with_override_text_color(at, g, fade(INK_DIM, 0.35));
            // the legend: how to look around the room, in the corner over it
            if self.options.pad || self.options.mouse {
                let st = Style::new(m.note(), m.faces.regular(), C8).spacing(0.02 * m.note()).line(1.55 * m.note());
                let g = galley(&ctx, "Look around", &st, f32::INFINITY);
                let at = pos2(rect.right() - px - g.size().x, rect.bottom() - bottom - g.size().y);
                let colour = fade(C8, 0.6);
                ui.painter().galley_with_override_text_color(at, g.clone(), colour);
                let icon = Rect::from_min_size(pos2(at.x - 0.4 * m.rem - 1.4 * m.rem, at.y + g.size().y / 2.0 - 0.7 * m.rem), vec2(1.4 * m.rem, 1.4 * m.rem));
                if self.options.pad {
                    paint_stick(ui.painter(), icon, colour, "R");
                } else {
                    paint_hand(ui.painter(), icon, colour);
                }
            }
        }
        if let Some((id, act)) = clicks.chosen {
            self.cursor = Some(id);
            self.on_cursor(s);
            self.error = None;
            self.perform(act);
        }
        if let Some((key, d)) = clicks.turn {
            self.cursor = Some(format!("setting:{key}"));
            self.on_cursor(s);
            self.turn_setting(&key, d);
        }
        let _ = clicks.hovered;
    }

    // ------------------------------------------------------------------ the keyboard

    /// The keys of this frame: the cursor moves (arrows, or the game's vim keys), Enter or Space
    /// chooses, Escape goes back, Page Up and Down walk the sections (or page a document), Y brings
    /// the roster's arrivals in. A text field keeps the keys it types.
    fn keys(&mut self, ui: &mut egui::Ui, s: &Screen) {
        let typing = self.typing(ui);
        let events = ui.input(|i| i.events.clone());
        for e in events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            // in the rc box every key is the text's but the one out of it, and Ctrl+S
            if self.rc.as_ref().is_some_and(|r| r.editing) && self.view == View::Rc {
                if key == egui::Key::Escape {
                    if let Some(r) = &mut self.rc {
                        r.editing = false;
                    }
                    ui.memory_mut(|m| m.surrender_focus(egui::Id::NULL.with("rc-text")));
                }
                if key == egui::Key::S && modifiers.command {
                    self.save_rc();
                }
                continue;
            }
            if self.view == View::Rc && key == egui::Key::S && modifiers.command {
                self.save_rc();
                continue;
            }
            if modifiers.ctrl || modifiers.alt || modifiers.command {
                continue;
            }
            if key == egui::Key::Tab {
                self.nav(if modifiers.shift { NavDir::Up } else { NavDir::Down });
                continue;
            }
            if let Some(d) = words::nav_dir(key, modifiers, typing) {
                self.nav(d);
                continue;
            }
            match key {
                egui::Key::Enter => {
                    if typing {
                        self.submit_form();
                    } else {
                        self.activate();
                    }
                }
                egui::Key::Space if !typing => self.activate(),
                egui::Key::Escape => {
                    ui.memory_mut(|m| {
                        if let Some(f) = m.focused() {
                            m.surrender_focus(f)
                        }
                    });
                    self.back();
                }
                egui::Key::Y if !typing && self.view == View::Watch => self.take_arrivals(),
                egui::Key::PageUp | egui::Key::PageDown if !typing => {
                    let d = if key == egui::Key::PageUp { -1 } else { 1 };
                    if matches!(self.view, View::Doc | View::News) {
                        let page = (self.doc_height - 48.0).max(48.0);
                        self.scroll_doc(d as f32 * page);
                    } else {
                        self.section(d);
                    }
                }
                egui::Key::Home | egui::Key::End if !typing && matches!(self.view, View::Doc | View::News) => {
                    self.doc_scroll = Some(if key == egui::Key::Home { 0.0 } else { f32::MAX / 4.0 });
                }
                egui::Key::F11 => self.perform(Act::Fullscreen),
                _ => {}
            }
        }
        let _ = s;
    }

    /// Whether a text field has the keys.
    fn typing(&self, ui: &egui::Ui) -> bool {
        let focused = ui.memory(|m| m.focused());
        [Field::Name, Field::Password, Field::Password2, Field::Email, Field::Address].iter().any(|f| Some(field_id(*f)) == focused)
    }

    // ------------------------------------------------------------------ the title screen

    fn draw_home(&mut self, ui: &mut egui::Ui, s: &Screen, m: &Metrics, clicks: &mut Clicks) {
        let ctx = ui.ctx().clone();
        let full = ui.max_rect();
        let width = (38.0 * m.rem).min(full.width());
        let gap = if m.short { 0.35 * m.rem } else { 0.65 * m.rem };
        // the head floats in the middle of the air above the menu: as far from the top as from Play
        let head = self.home_head(&ctx, s, m, width);
        let notices: Vec<_> = s.notices.iter().map(|n| self.notice_galley(&ctx, n, m, width)).collect();
        let rows_h: f32 = self.rows_height(&ctx, s, m, width);
        let help_h = self.help_height(&ctx, s, m, width);
        let footer_h = 0.75 * m.rem + self.utilities_height(&ctx, s, m, width);
        let notices_h: f32 = notices.iter().map(|g| g.size().y + gap).sum();
        let below = notices_h + rows_h + gap + help_h + gap + footer_h;
        let total = head.height + gap + below;
        let mut draw = |f: &mut Self, ui: &mut egui::Ui, origin: egui::Pos2, height: f32| {
            let air = (height - below - head.height).max(gap);
            head.paint(ui, pos2(origin.x, origin.y + air / 2.0), m);
            let mut y = origin.y + (height - below).max(head.height + gap);
            for (g, n) in notices.iter().zip(&s.notices) {
                f.paint_notice(ui, n, g.clone(), pos2(origin.x, y), m, width, clicks);
                y += g.size().y + gap;
            }
            y = f.paint_rows(ui, s, m, pos2(origin.x, y), width, clicks);
            y += gap;
            f.paint_help(ui, m, pos2(origin.x, y), width, help_h);
            y += help_h + gap + 0.75 * m.rem;
            f.paint_utilities(ui, s, m, pos2(origin.x, y), width, clicks);
        };
        if total <= full.height() {
            draw(self, ui, full.min, full.height());
        } else {
            // a window too short for the whole title screen: it scrolls, the head on top
            let scroll_to = std::mem::take(&mut self.scroll_to_cursor);
            egui::ScrollArea::vertical().id_salt("home-scroll").max_height(full.height()).show(ui, |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(full.width(), total), Sense::hover());
                draw(self, ui, r.min, total);
                if scroll_to {
                    if let Some(c) = self.cursor_rect {
                        ui.scroll_to_rect(c, None);
                    }
                }
            });
        }
    }

    fn home_head(&self, ctx: &egui::Context, s: &Screen, m: &Metrics, width: f32) -> Head {
        let size = if m.short { 4.5 * m.rem } else { (0.13 * m.w).clamp(4.5 * m.rem, 7.5 * m.rem) };
        let line = if m.short { 1.0 } else { 1.1 };
        let title = galley(ctx, &s.title, &Style::new(size, m.faces.title(), INK).line(size * line), f32::INFINITY);
        let orb = 0.71 * size;
        let lede_size = if m.short { 0.9 * m.rem } else if m.compact { m.rem } else { m.note() };
        let lede_w = if m.short { width } else { (34.0 * m.rem).min(width) };
        let lede = s.lede.as_ref().map(|l| galley(ctx, l, &Style::new(lede_size, m.faces.regular(), INK_DIM).spacing(0.02 * lede_size).line(1.55 * lede_size), lede_w));
        let splash = if m.short {
            None
        } else {
            s.splash.map(|t| {
                let st = Style::new(m.rem, m.faces.text(), fade(INK_DIM, 0.85)).line(1.5 * m.rem);
                galley_runs(ctx, &[("“", &st.clone().italic()), (t, &st.clone().italic()), ("” — Xom", &st)], (40.0 * 0.602 * m.rem).min(width))
            })
        };
        let title_w = orb + 0.16 * size + title.size().x;
        let (lede_gap, splash_gap) = (if m.short { 0.0 } else if m.compact { 0.4 * m.rem } else { 0.75 * m.rem }, m.rem);
        let height = if m.short {
            title.size().y.max(lede.as_ref().map(|l| l.size().y).unwrap_or(0.0))
        } else {
            title.size().y + lede.as_ref().map(|l| lede_gap + l.size().y).unwrap_or(0.0) + splash.as_ref().map(|s| splash_gap + s.size().y).unwrap_or(0.0)
        };
        Head { title, orb, size, title_w, lede, lede_gap, splash, splash_gap, height, inline: m.short }
    }

    // ------------------------------------------------------------------ list screens

    fn draw_list(&mut self, ui: &mut egui::Ui, s: &Screen, m: &Metrics, clicks: &mut Clicks) {
        let ctx = ui.ctx().clone();
        let full = ui.max_rect();
        let besides = s.rows.iter().map(|r| r.also.len()).max().unwrap_or(0);
        let width = match s.cls {
            Cls::Doc | Cls::Watch => {
                if s.cls == Cls::Watch {
                    full.width()
                } else {
                    (60.0 * m.rem).min(full.width())
                }
            }
            Cls::News => (48.0 * m.rem).min(full.width()),
            Cls::Settings => (44.0 * m.rem).min(full.width()),
            _ if besides >= 2 => (48.0 * m.rem).min(full.width()),
            _ => (38.0 * m.rem).min(full.width()),
        };
        let gap = 0.6 * m.rem;
        // the head: the screen's name in the wordmark's face, the line under it in small caps
        let title_size = if m.narrow { m.fs.min(1.6 * m.rem) } else { (0.06 * m.w).clamp(2.4 * m.rem, 3.2 * m.rem) };
        let title = galley(&ctx, &s.title, &Style::new(title_size, m.faces.title(), GOLD_BRIGHT).line(1.05 * title_size), width);
        let lede_size = if m.narrow { m.fs.min(1.05 * m.rem) } else { 0.85 * m.rem };
        let lede = s.lede.as_ref().map(|l| galley(&ctx, &upper(l), &Style::new(lede_size, m.faces.bold(), INK_DIM).spacing(if m.narrow { 0.0 } else { 0.14 * lede_size }).line(1.5 * lede_size), width));
        let mut y = full.top();
        let x = full.left();
        shadowed(ui.painter(), pos2(x, y), &title, GOLD_BRIGHT, 0.6);
        y += title.size().y;
        if let Some(l) = &lede {
            y += 0.45 * m.rem;
            shadowed(ui.painter(), pos2(x, y), l, INK_DIM, 0.6);
            y += l.size().y;
        }
        y += 0.6 * m.rem + gap;
        for n in &s.notices {
            let g = self.notice_galley(&ctx, n, m, width);
            let h = g.size().y + if n.ask { 3.0 * m.rem } else { 0.0 };
            self.paint_notice(ui, n, g, pos2(x, y), m, width, clicks);
            y += h + gap;
        }
        // the rows, in a region of their own when they would not fit the window
        let help_h = self.help_height(&ctx, s, m, width).max(1.5 * m.note());
        let rows_h = self.rows_height(&ctx, s, m, width);
        let foot = 0.8 * m.rem + help_h;
        let room_for_rows = full.bottom() - y - foot;
        let fixed_below = matches!(s.below, Below::None);
        if rows_h > room_for_rows && fixed_below {
            let area = Rect::from_min_max(pos2(x, y), pos2(full.right(), full.bottom() - foot));
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(area));
            let scroll_to = std::mem::take(&mut self.scroll_to_cursor);
            egui::ScrollArea::vertical().id_salt(("rows", format!("{:?}", self.view))).max_height(area.height()).show(&mut c, |ui| {
                let at = ui.cursor().min;
                let (r, _) = ui.allocate_exact_size(vec2(width, rows_h), Sense::hover());
                let _ = r;
                let cursor_rect = self.paint_rows(ui, s, m, at, width, clicks);
                let _ = cursor_rect;
                if scroll_to {
                    if let Some(r) = self.cursor_rect {
                        ui.scroll_to_rect(r, None);
                    }
                }
            });
            y = area.bottom();
        } else {
            y = self.paint_rows(ui, s, m, pos2(x, y), width, clicks);
        }
        // what stands under the rows takes the room left over the message line
        let region = Rect::from_min_max(pos2(x, y + gap + 0.6 * m.rem), pos2(x + width, full.bottom() - foot));
        let used = match &s.below {
            Below::None => 0.0,
            Below::Doc(body) => self.paint_doc(ui, body, m, region),
            Below::News => self.paint_news(ui, m, region, clicks),
            Below::Roster => self.paint_roster(ui, m, region, clicks),
            Below::Settings(group) => self.paint_settings(ui, group, m, region, clicks),
            Below::Controls => self.paint_controls(ui, m, region),
        };
        if used > 0.0 {
            y = region.top() + used;
        }
        if !s.links.is_empty() {
            y += 0.6 * m.rem;
            for (k, (label, href)) in s.links.iter().enumerate() {
                let id = format!("link:{k}");
                let r = Rect::from_min_size(pos2(x, y), vec2(width, m.row_height()));
                let mut row = Row::new(id.clone(), format!("{label} ↗"), "+", "", Act::Open(href.clone()));
                row.sub = None;
                self.paint_row(ui, &row, r, m, clicks, 0.0);
                y += m.row_height();
            }
        }
        y += 0.8 * m.rem;
        self.paint_help(ui, m, pos2(x, y), width, help_h);
    }

    /// The exit report: a card over the home screen, in the middle of the window.
    fn draw_dialog(&mut self, ui: &mut egui::Ui, s: &Screen, m: &Metrics, frame: Rect, clicks: &mut Clicks) {
        let ctx = ui.ctx().clone();
        ui.painter().rect_filled(ui.ctx().content_rect(), 0.0, rgba(3, 2, 6, 0.6));
        let pad = vec2(1.6 * m.rem, 1.4 * m.rem);
        let words: Vec<Arc<egui::Galley>> = s
            .notices
            .iter()
            .map(|n| {
                if n.hint {
                    galley(&ctx, &n.text, &m.text(m.rem, C14), (60.0 * 0.602 * m.rem).min(frame.width()))
                } else {
                    galley(&ctx, &n.text, &m.text(m.rem, C14), f32::INFINITY)
                }
            })
            .collect();
        let content_w = words.iter().map(|g| g.size().x).fold((38.0 * m.rem).min(frame.width() - 2.0 * pad.x), f32::max).min(frame.width() - 2.0 * pad.x);
        let title = galley(&ctx, &s.title, &Style::new((0.06 * m.w).clamp(2.4 * m.rem, 3.2 * m.rem), m.faces.title(), GOLD_BRIGHT), content_w);
        let rows_h = s.rows.len() as f32 * (m.row_height() + 0.3 * m.rem);
        let words_h: f32 = words.iter().map(|g| g.size().y + 0.6 * m.rem).sum();
        let h = title.size().y + 1.2 * m.rem + words_h + rows_h + 2.0 * pad.y + 2.0 * m.rem;
        let card = Rect::from_center_size(frame.center(), vec2(content_w + 2.0 * pad.x, h.min(frame.height())));
        ui.painter().rect_filled(card.expand(8.0).translate(vec2(0.0, 1.5 * m.rem)), 0.0, rgba(0, 0, 0, 0.3));
        gradient(ui.painter(), card, false, &[(0.0, rgba(11, 9, 17, 0.97)), (1.0, rgba(6, 5, 11, 0.97))]);
        ui.painter().rect_stroke(card, 0.0, egui::Stroke::new(1.0, rgba(216, 180, 95, 0.3)), egui::StrokeKind::Inside);
        let x = card.left() + pad.x;
        let mut y = card.top() + pad.y;
        ui.painter().galley(pos2(x, y), title.clone(), GOLD_BRIGHT);
        y += title.size().y + 1.2 * m.rem;
        for g in &words {
            ui.painter().with_clip_rect(card).galley(pos2(x, y), g.clone(), C14);
            y += g.size().y + 0.6 * m.rem;
        }
        self.paint_rows(ui, s, m, pos2(x, y), content_w, clicks);
    }

    // ------------------------------------------------------------------ rows

    /// One row's height at `width`: its label and the line under it.
    fn row_height(&self, ctx: &egui::Context, r: &Row, m: &Metrics, width: f32) -> f32 {
        if r.heading {
            return (if r.gap { 0.9 * m.rem } else { 0.0 }) + 0.2 * m.rem + 1.5 * 0.75 * m.rem;
        }
        if r.text {
            return (22.0 * m.rem).min(0.45 * m.h) + 1.0 * m.rem;
        }
        let gap = if r.gap { 0.7 * m.rem } else { 0.0 };
        if r.input.is_some() {
            return gap + (0.7 * m.rem * 2.0 + m.action() * 1.2 + 0.3 * m.rem + 44.0).max(m.row_height());
        }
        let text_w = width - m.label_inset() - m.rem;
        let label = galley(ctx, &r.label, &m.text(m.action(), C7), text_w);
        let sub = r.sub.as_ref().map(|t| galley(ctx, t, &m.text(m.note(), INK_DIM), text_w));
        let inner = label.size().y + sub.map(|s| 0.3 * m.rem + s.size().y).unwrap_or(0.0);
        gap + (inner + 1.4 * m.rem).max(m.row_height())
    }

    fn rows_height(&self, ctx: &egui::Context, s: &Screen, m: &Metrics, width: f32) -> f32 {
        let main_w = width - self.beside_width(ctx, s, m);
        let under = m.phone && s.rows.iter().map(|r| r.also.len()).max().unwrap_or(0) >= 2;
        let beside_h = 44.0f32.max(m.note() * 1.6);
        s.rows.iter().map(|r| self.row_height(ctx, r, m, main_w) + 0.3 * m.rem + if under && !r.also.is_empty() { beside_h } else { 0.0 }).sum::<f32>() - if s.rows.is_empty() { 0.0 } else { 0.3 * m.rem }
    }

    /// The width the columns beside the rows take (each as wide as its widest label).
    fn beside_width(&self, ctx: &egui::Context, s: &Screen, m: &Metrics) -> f32 {
        let cols = s.rows.iter().map(|r| r.also.len()).max().unwrap_or(0);
        if cols == 0 || (m.phone && cols >= 2) {
            return 0.0;
        }
        (0..cols).map(|k| s.rows.iter().filter_map(|r| r.also.get(k)).map(|a| galley(ctx, &a.label, &m.text(m.note(), C8), f32::INFINITY).size().x + m.rem + 0.2 * m.rem).fold(0.0, f32::max)).sum()
    }

    /// Paint the rows from `at`; the bottom they reach comes back.
    pub(crate) fn paint_rows(&mut self, ui: &mut egui::Ui, s: &Screen, m: &Metrics, at: egui::Pos2, width: f32, clicks: &mut Clicks) -> f32 {
        let ctx = ui.ctx().clone();
        let beside_w = self.beside_width(&ctx, s, m);
        let cols = s.rows.iter().map(|r| r.also.len()).max().unwrap_or(0);
        let col_w: Vec<f32> = (0..cols).map(|k| s.rows.iter().filter_map(|r| r.also.get(k)).map(|a| galley(&ctx, &a.label, &m.text(m.note(), C8), f32::INFINITY).size().x + m.rem).fold(0.0, f32::max)).collect();
        let main_w = width - beside_w;
        let mut y = at.y;
        for (i, r) in s.rows.iter().enumerate() {
            if i > 0 {
                y += 0.3 * m.rem;
            }
            let h = self.row_height(&ctx, r, m, main_w);
            let gap = if r.gap && !r.heading { 0.7 * m.rem } else { 0.0 };
            if r.heading {
                let top = y + if r.gap { 0.9 * m.rem } else { 0.0 } + 0.2 * m.rem;
                let g = galley(&ctx, &upper(&r.label), &Style::new(0.75 * m.rem, m.faces.bold(), INK_DIM).spacing(0.14 * 0.75 * m.rem).line(1.5 * 0.75 * m.rem), f32::INFINITY);
                shadowed(ui.painter(), pos2(at.x + 0.7 * m.rem, top), &g, INK_DIM, 0.6);
                y += h;
                continue;
            }
            let rect = Rect::from_min_size(pos2(at.x, y + gap), vec2(main_w, h - gap));
            if r.text {
                self.paint_rc_box(ui, r, rect, m, clicks);
            } else if let Some(f) = r.input {
                self.paint_field(ui, r, f, rect, m, clicks);
            } else {
                self.paint_row(ui, r, rect, m, clicks, 0.0);
            }
            // what stands beside the row, a column each; a phone puts two or more on a line under it,
            // lined up with the text, so the name keeps the width it needs
            let beside_h = 44.0f32.max(m.note() * 1.6);
            let under = m.phone && cols >= 2;
            let mut bx = if under { at.x + m.label_inset() - 0.5 * m.rem } else { at.x + main_w + 0.2 * m.rem };
            for (k, a) in r.also.iter().enumerate() {
                let w = if under { galley(&ctx, &a.label, &m.text(m.note(), C8), f32::INFINITY).size().x + m.rem } else { col_w.get(k).copied().unwrap_or(0.0) };
                let top = if under { rect.bottom() } else { rect.top() + (rect.height() - beside_h) / 2.0 };
                let br = Rect::from_min_size(pos2(bx, top), vec2(w, beside_h));
                self.paint_beside(ui, a, br, m, clicks);
                bx += w + 0.2 * m.rem;
            }
            y += h;
            if under && !r.also.is_empty() {
                y += beside_h;
            }
        }
        y
    }

    /// A row as a button: the marker in the margin, the label, the note under it; the cursor's bar
    /// and its `@` when it stands here.
    pub(crate) fn paint_row(&mut self, ui: &mut egui::Ui, r: &Row, rect: Rect, m: &Metrics, clicks: &mut Clicks, _indent: f32) {
        let ctx = ui.ctx().clone();
        let id = egui::Id::NULL.with(("row", &r.id));
        let resp = ui.interact(rect, id, Sense::click());
        let focused = self.current() == Some(r.id.as_str());
        if focused {
            self.cursor_rect = Some(rect);
        }
        if resp.hovered() && !r.off {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            clicks.hovered = Some(r.id.clone());
        }
        if resp.clicked() {
            clicks.chosen = Some((r.id.clone(), r.act.clone()));
        }
        let alpha = if r.off { 0.45 } else { 1.0 };
        let p = ui.painter().clone();
        if focused {
            gradient(&p, rect, true, &[(0.0, rgba(216, 180, 95, 0.2)), (0.55, rgba(216, 180, 95, 0.05)), (1.0, rgba(216, 180, 95, 0.0))]);
        }
        let colour = fade(if focused { GOLD_BRIGHT } else if resp.hovered() { INK } else { C7 }, alpha);
        let text_w = rect.width() - m.label_inset() - m.rem;
        let label = galley(&ctx, &r.label, &m.text(m.action(), colour), text_w);
        let sub = r.sub.as_ref().map(|t| galley(&ctx, t, &m.text(m.note(), fade(INK_DIM, alpha)), text_w));
        let inner = label.size().y + sub.as_ref().map(|s| 0.3 * m.rem + s.size().y).unwrap_or(0.0);
        let top = rect.top() + (rect.height() - inner) / 2.0;
        let lx = rect.left() + m.label_inset();
        shadowed(&p, pos2(lx, top), &label, colour, 0.5);
        // the glyph in the margin, as the game marks a feature; the @ (the cursor) steps in front of it
        let mid = top + label.size().y / 2.0;
        let mark = if focused { "@" } else { r.marker };
        if !mark.is_empty() {
            let mc = if focused { GOLD_BRIGHT } else { fade(C8, alpha) };
            let g = galley(&ctx, mark, &Style::new(m.action(), m.faces.bold(), mc), f32::INFINITY);
            p.galley(pos2(rect.left() + m.cursor_inset(), mid - g.size().y / 2.0), g, mc);
        }
        let mut after = lx + label.rows.last().map(|row| row.rect().right()).unwrap_or(label.size().x);
        // the chosen account's dot: after its name, and only when something is not right yet
        if let Some(c) = r.conn.filter(|c| matches!(c, Conn::Wait | Conn::Down)) {
            let d = 0.5 * m.action();
            let colour = if c == Conn::Down { C12 } else { fade(GOLD, 0.6 + 0.4 * (isorun_client::platform::seconds() as f32 * std::f32::consts::TAU / 1.4).cos().abs()) };
            let centre = pos2(after + 0.6 * m.action() + d / 2.0, top + label.size().y - label.size().y / 2.0);
            p.circle_filled(centre, d * 0.9, fade(colour, 0.25));
            p.circle_filled(centre, d / 2.0, colour);
            after += 0.6 * m.action() + d;
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        // something not yet seen: a dot in the cursor's gold
        if r.dot {
            let d = 0.5 * m.action();
            let centre = pos2(after + 0.45 * m.action() + d / 2.0, top + label.size().y / 2.0);
            p.circle_filled(centre, d * 0.8, fade(ACCENT, 0.12));
            p.circle_filled(centre, d / 2.0, ACCENT);
        }
        if let Some(sg) = sub {
            let sc = if focused { fade(C7, alpha) } else { fade(INK_DIM, alpha) };
            shadowed(&p, pos2(lx, top + label.size().y + 0.3 * m.rem), &sg, sc, 0.5);
        }
    }

    fn paint_beside(&mut self, ui: &mut egui::Ui, a: &Beside, rect: Rect, m: &Metrics, clicks: &mut Clicks) {
        let resp = ui.interact(rect, egui::Id::NULL.with(("beside", &a.id)), Sense::click());
        let focused = self.current() == Some(a.id.as_str());
        if resp.clicked() {
            clicks.chosen = Some((a.id.clone(), a.act.clone()));
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let p = ui.painter();
        if focused {
            gradient(p, rect, true, &[(0.0, rgba(216, 180, 95, 0.2)), (0.55, rgba(216, 180, 95, 0.05)), (1.0, rgba(216, 180, 95, 0.0))]);
            p.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, GOLD), egui::StrokeKind::Inside);
        }
        let colour = if focused { GOLD_BRIGHT } else if resp.hovered() { C15 } else { C8 };
        let g = galley(ui.ctx(), &a.label, &m.text(m.note(), colour), f32::INFINITY);
        p.galley(pos2(rect.left() + 0.5 * m.rem, rect.center().y - g.size().y / 2.0), g, colour);
    }

    /// A text field as a row: the prompt, the field under it.
    fn paint_field(&mut self, ui: &mut egui::Ui, r: &Row, f: Field, rect: Rect, m: &Metrics, clicks: &mut Clicks) {
        let ctx = ui.ctx().clone();
        let focused = self.current() == Some(r.id.as_str());
        let p = ui.painter().clone();
        if focused {
            gradient(&p, rect, true, &[(0.0, rgba(216, 180, 95, 0.2)), (0.55, rgba(216, 180, 95, 0.05)), (1.0, rgba(216, 180, 95, 0.0))]);
            self.cursor_rect = Some(rect);
        }
        let colour = if focused { GOLD_BRIGHT } else { C7 };
        let label = galley(&ctx, &r.label, &m.text(m.action(), colour), rect.width());
        let top = rect.top() + 0.7 * m.rem;
        p.galley(pos2(rect.left() + m.label_inset(), top), label.clone(), colour);
        if focused {
            let g = galley(&ctx, "@", &Style::new(m.action(), m.faces.bold(), GOLD_BRIGHT), f32::INFINITY);
            p.galley(pos2(rect.left() + m.cursor_inset(), top + (label.size().y - g.size().y) / 2.0), g, GOLD_BRIGHT);
        }
        let input_rect = Rect::from_min_max(pos2(rect.left() + m.label_inset(), top + label.size().y + 0.3 * m.rem), pos2(rect.right() - m.rem, top + label.size().y + 0.3 * m.rem + 44.0));
        let placeholder = match f {
            Field::Name => "name",
            Field::Password => "password",
            Field::Password2 => "repeat password",
            Field::Email => "email (optional)",
            Field::Address => "crawl.example.org",
        };
        let value = self.fields.entry(f).or_default();
        let edit = egui::TextEdit::singleline(value)
            .id(field_id(f))
            .password(matches!(f, Field::Password | Field::Password2))
            .hint_text(egui::RichText::new(placeholder).color(C8))
            .font(egui::FontId::new(m.note(), m.faces.text()))
            .text_color(C15)
            .frame(egui::Frame::NONE)
            .align(egui::Align2::LEFT_CENTER)
            .desired_width(input_rect.width());
        let resp = ui.put(input_rect, edit);
        let line = if resp.has_focus() { C7 } else { rgba(191, 191, 191, 0.25) };
        p.line_segment([input_rect.left_bottom(), input_rect.right_bottom()], egui::Stroke::new(1.0, line));
        // the cursor on the row gives its field the caret; a click into the field puts the cursor here
        if resp.gained_focus() || resp.clicked() {
            if self.cursor.as_deref() != Some(r.id.as_str()) {
                clicks.chosen = Some((r.id.clone(), Act::None));
            }
        }
        if focused && !resp.has_focus() && !ctx.memory(|mm| mm.focused().is_some_and(|f| f != resp.id)) {
            resp.request_focus();
        }
        if !focused && resp.has_focus() && self.cursor.as_deref() != Some(r.id.as_str()) && !resp.gained_focus() {
            resp.surrender_focus();
        }
        let whole = ui.interact(Rect::from_min_max(rect.min, pos2(rect.right(), input_rect.top())), egui::Id::NULL.with(("field", &r.id)), Sense::click());
        if whole.clicked() {
            clicks.chosen = Some((r.id.clone(), Act::None));
            resp.request_focus();
        }
    }

    // ------------------------------------------------------------------ the head, notices and help

    fn notice_galley(&self, ctx: &egui::Context, n: &Notice, m: &Metrics, width: f32) -> Arc<egui::Galley> {
        let w = (width - m.label_inset()).min(60.0 * 0.602 * m.note());
        let colour = if n.hint { INK_DIM } else { INK };
        galley(ctx, &n.text, &m.text(m.note(), colour).line(1.5 * m.note()), w)
    }

    /// A notice or an error: a line in the menu's own voice with a `!` in the cursor's column, gold
    /// for a notice and a muted red for an error; Yes and No under a question.
    fn paint_notice(&mut self, ui: &mut egui::Ui, n: &Notice, g: Arc<egui::Galley>, at: egui::Pos2, m: &Metrics, _width: f32, clicks: &mut Clicks) {
        let p = ui.painter().clone();
        let x = if n.hint { at.x } else { at.x + m.label_inset() };
        if !n.hint {
            let mc = if n.error { ERROR_MARK } else { GOLD };
            let mark = galley(ui.ctx(), "!", &Style::new(m.note(), m.faces.bold(), mc), f32::INFINITY);
            p.galley(pos2(at.x + m.cursor_inset(), at.y), mark, mc);
        }
        let colour = if n.hint { INK_DIM } else { INK };
        shadowed(&p, pos2(x, at.y), &g, colour, 0.5);
        if n.ask {
            let y = at.y + g.size().y;
            for (k, (id, label, yes)) in [("terminate:yes", "Yes", true), ("terminate:no", "No", false)].into_iter().enumerate() {
                let r = Rect::from_min_size(pos2(x + k as f32 * 7.0 * m.rem, y), vec2(6.0 * m.rem, 2.8 * m.rem));
                let row = Row::new(id, label, "", "", Act::Terminate(yes));
                self.paint_row(ui, &row, r, m, clicks, 0.0);
            }
        }
    }

    fn help_height(&self, ctx: &egui::Context, s: &Screen, m: &Metrics, width: f32) -> f32 {
        let w = width.min(60.0 * 0.602 * m.note());
        let line = 1.5 * m.note();
        if s.cls == Cls::Home {
            // the line is sized for every hint, not just the one under the cursor, so the menu never moves
            let tallest = s.rows.iter().chain(s.utilities.iter()).map(|r| {
                let mut h = galley(ctx, &r.hint, &m.text(m.note(), INK_DIM).line(line), w).size().y;
                if let Some(f) = &r.fine {
                    h += galley(ctx, f, &m.text(0.8 * m.note(), C8), w).size().y + 0.2 * 0.8 * m.note();
                }
                h
            });
            let min = if m.short { 1.5 } else if m.compact { 2.4 } else { 3.0 } * m.note();
            return tallest.fold(min, f32::max);
        }
        let (text, fine, _) = self.said();
        let mut h = galley(ctx, text, &m.text(m.note(), INK_DIM), w).size().y;
        if let Some(f) = fine {
            h += galley(ctx, f, &m.text(0.8 * m.note(), C8), w).size().y;
        }
        h.max(1.5 * m.note())
    }

    /// The message line: what the row under the cursor is; it fades in as it changes.
    fn paint_help(&self, ui: &mut egui::Ui, m: &Metrics, at: egui::Pos2, width: f32, _h: f32) {
        let (text, fine, since) = self.said();
        let t = ((isorun_client::platform::seconds() - since) / 0.26).clamp(0.0, 1.0) as f32;
        if t < 1.0 {
            ui.ctx().request_repaint();
        }
        let w = width.min(60.0 * 0.602 * m.note());
        let a = if self.room.reduced_motion { 1.0 } else { t };
        let dy = 3.0 * (1.0 - t);
        let g = galley(ui.ctx(), text, &m.text(m.note(), INK_DIM).line(1.5 * m.note()), w);
        let y = at.y + dy;
        ui.painter().galley_with_override_text_color(pos2(at.x, y), g.clone(), fade(INK_DIM, a));
        if let Some(f) = fine {
            let fg = galley(ui.ctx(), f, &m.text(0.8 * m.note(), C8), w);
            ui.painter().galley_with_override_text_color(pos2(at.x, y + g.size().y + 0.2 * 0.8 * m.note()), fg, fade(C8, 0.7 * a));
        }
    }

    // ------------------------------------------------------------------ the footer

    fn utility_galleys(&self, ctx: &egui::Context, s: &Screen, m: &Metrics) -> Vec<(Arc<egui::Galley>, f32)> {
        let pad_l = 0.3 * m.rem + 0.6 * m.note() + m.label_inset() - m.cursor_inset() - 0.6 * m.action();
        s.utilities
            .iter()
            .map(|u| {
                let g = galley(ctx, &u.label, &m.text(m.note(), INK_DIM), f32::INFINITY);
                let extra = if u.dot { 0.95 * m.note() } else { 0.0 } + if u.conn.is_some_and(|c| matches!(c, Conn::Wait | Conn::Down)) { 1.1 * m.note() } else { 0.0 };
                let sub = u.sub.as_ref().map(|t| galley(ctx, t, &m.text(m.note(), INK_DIM), f32::INFINITY).size().x + m.rem).unwrap_or(0.0);
                let w = pad_l + g.size().x + extra + sub + 0.5 * m.rem;
                (g, w)
            })
            .collect()
    }

    fn utilities_height(&self, ctx: &egui::Context, s: &Screen, m: &Metrics, width: f32) -> f32 {
        let items = self.utility_galleys(ctx, s, m);
        let h = 44.0f32.max(m.note() * 1.2 + 1.3 * m.rem);
        let mut lines = 1;
        let mut x = 0.0;
        for (i, (_, w)) in items.iter().enumerate() {
            // a phone gives the account a line of its own
            let own = m.phone && s.utilities.get(i).is_some_and(|u| u.id == "account");
            if (x > 0.0 && x + w > width) || (i > 0 && m.phone && s.utilities.get(i - 1).is_some_and(|u| u.id == "account")) {
                lines += 1;
                x = 0.0;
            }
            x += w + m.rem;
            if own {
                x = width;
            }
        }
        lines as f32 * h
    }

    /// The account and the information: one quiet line of chips under the menu.
    fn paint_utilities(&mut self, ui: &mut egui::Ui, s: &Screen, m: &Metrics, at: egui::Pos2, width: f32, clicks: &mut Clicks) {
        let ctx = ui.ctx().clone();
        let items = self.utility_galleys(&ctx, s, m);
        let h = 44.0f32.max(m.note() * 1.2 + 1.3 * m.rem);
        let pad_l = 0.3 * m.rem + 0.6 * m.note() + m.label_inset() - m.cursor_inset() - 0.6 * m.action();
        let (mut x, mut y) = (0.0, at.y);
        for (i, ((_, w), u)) in items.iter().zip(&s.utilities).enumerate() {
            if (x > 0.0 && x + w > width) || (i > 0 && m.phone && s.utilities[i - 1].id == "account") {
                x = 0.0;
                y += h;
            }
            let rect = Rect::from_min_size(pos2(at.x + x, y), vec2(*w, h));
            x += w + m.rem;
            let resp = ui.interact(rect, egui::Id::NULL.with(("utility", &u.id)), Sense::click());
            if resp.clicked() {
                clicks.chosen = Some((u.id.clone(), u.act.clone()));
            }
            let focused = self.current() == Some(u.id.as_str());
            if focused {
                self.cursor_rect = Some(rect);
            }
            if resp.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let colour = if focused || resp.hovered() { GOLD_BRIGHT } else { INK_DIM };
            let p = ui.painter();
            if focused {
                gradient(p, rect, true, &[(0.0, rgba(216, 180, 95, 0.2)), (0.55, rgba(216, 180, 95, 0.05)), (1.0, rgba(216, 180, 95, 0.0))]);
            }
            let g = galley(&ctx, &u.label, &m.text(m.note(), colour), f32::INFINITY);
            let ty = rect.center().y - g.size().y / 2.0;
            let mark = if focused { "@" } else { u.marker };
            let mc = if focused { GOLD_BRIGHT } else { C8 };
            let mg = galley(&ctx, mark, &Style::new(m.note(), m.faces.bold(), mc), f32::INFINITY);
            p.galley(pos2(rect.left() + 0.3 * m.rem, rect.center().y - mg.size().y / 2.0), mg, mc);
            shadowed(p, pos2(rect.left() + pad_l, ty), &g, colour, 0.5);
            let mut after = rect.left() + pad_l + g.size().x;
            if let Some(c) = u.conn.filter(|c| matches!(c, Conn::Wait | Conn::Down)) {
                let d = 0.5 * m.note();
                let colour = if c == Conn::Down { C12 } else { fade(GOLD, 0.6 + 0.4 * (isorun_client::platform::seconds() as f32 * std::f32::consts::TAU / 1.4).cos().abs()) };
                p.circle_filled(pos2(after + 0.6 * m.note() + d / 2.0, rect.center().y), d / 2.0, colour);
                after += 0.6 * m.note() + d;
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
            if u.dot {
                let d = 0.5 * m.note();
                let centre = pos2(after + 0.45 * m.note() + d / 2.0, rect.center().y);
                p.circle_filled(centre, d * 0.8, fade(ACCENT, 0.12));
                p.circle_filled(centre, d / 2.0, ACCENT);
            }
            if let Some(sub) = &u.sub {
                let sg = galley(&ctx, sub, &m.text(m.note(), INK_DIM), f32::INFINITY);
                p.galley(pos2(after + m.rem, ty), sg, INK_DIM);
            }
        }
    }
}

/// The id a field's text box has.
pub(crate) fn field_id(f: Field) -> egui::Id {
    egui::Id::NULL.with(("front-field", f))
}

/// The title screen's head: the orb and the wordmark, the line under it, Xom's word.
pub(crate) struct Head {
    title: Arc<egui::Galley>,
    orb: f32,
    size: f32,
    title_w: f32,
    lede: Option<Arc<egui::Galley>>,
    lede_gap: f32,
    splash: Option<Arc<egui::Galley>>,
    splash_gap: f32,
    pub height: f32,
    inline: bool,
}

impl Head {
    fn paint(&self, ui: &egui::Ui, at: egui::Pos2, m: &Metrics) {
        let p = ui.painter();
        // the Orb of Zot's own tile before the name, its core the O's cap height, on the baseline
        let tex = orb_texture(ui.ctx());
        let baseline = at.y + self.title.rows.first().map(|r| r.rect().top() + r.row.size.y * 0.8).unwrap_or(self.size);
        let orb = Rect::from_min_size(pos2(at.x, baseline - self.orb + 0.055 * self.size), vec2(self.orb, self.orb));
        // drop-shadow(0 0 8px orange) drop-shadow(0 0 18px violet): a soft glow round the orb
        radial(p, orb.expand(0.25 * self.orb), orb.center(), &[(0.0, rgba(185, 120, 31, 0.3)), (0.45, rgba(139, 63, 176, 0.12)), (0.7, rgba(139, 63, 176, 0.0)), (1.0, rgba(0, 0, 0, 0.0))]);
        p.image(tex.id(), orb, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        let tx = at.x + self.orb + 0.16 * self.size;
        shadowed(p, pos2(tx, at.y), &self.title, INK, 0.7);
        if let Some(l) = &self.lede {
            if self.inline {
                let ly = baseline - l.size().y * 0.8;
                shadowed(p, pos2(at.x + self.title_w + m.rem, ly), l, INK_DIM, 0.5);
            } else {
                shadowed(p, pos2(at.x, at.y + self.title.size().y + self.lede_gap), l, INK_DIM, 0.5);
            }
        }
        if let (Some(s), false) = (&self.splash, self.inline) {
            let y = at.y + self.title.size().y + self.lede.as_ref().map(|l| self.lede_gap + l.size().y).unwrap_or(0.0) + self.splash_gap;
            p.galley(pos2(at.x, y), s.clone(), INK_DIM);
        }
    }
}

/// The Orb of Zot's tile (`public/orb.png`), as a texture, once.
fn orb_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::NULL.with("isorun-orb");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t;
    }
    let img = image::load_from_memory_with_format(include_bytes!("../../../../apps/isorun/public/orb.png"), image::ImageFormat::Png).map(|i| i.to_rgba8()).ok();
    let colour = match img {
        Some(i) => egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw()),
        None => egui::ColorImage::filled([1, 1], Color32::TRANSPARENT),
    };
    let t = ctx.load_texture("isorun-orb", colour, egui::TextureOptions::NEAREST);
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

#[allow(dead_code)]
fn _align(_: Align2) {}
