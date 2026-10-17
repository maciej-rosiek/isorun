//! What stands under a screen's rows, each in a region of its own that scrolls, so the head above
//! and the message line under it keep their place: a document, What's new's releases, the roster,
//! a settings page, the gamepad sheet, and the rc file's box.

use crate::front::*;
use crate::layout::{Clicks, Metrics};
use crate::markdown::{self, DocStyle};
use crate::paint::*;
use crate::settings;
use crate::words;
use egui::{pos2, vec2, Color32, Rect, Sense};

fn highlight(p: &egui::Painter, rect: Rect) {
    gradient(p, rect, true, &[(0.0, rgba(216, 180, 95, 0.2)), (0.55, rgba(216, 180, 95, 0.05)), (1.0, rgba(216, 180, 95, 0.0))]);
}

impl FrontEnd {
    /// A scrolling region in `rect`; `content` draws into it at the width it is given and says how
    /// tall it came out. Returns the height the region took.
    fn region(&mut self, ui: &mut egui::Ui, rect: Rect, salt: &str, panel: bool, pad: egui::Vec2, scroll: Option<f32>, content: &mut dyn FnMut(&mut Self, &mut egui::Ui, f32)) -> (f32, f32, f32) {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
        child.spacing_mut().item_spacing = vec2(0.0, 0.0);
        // `scrollbar-width: thin`, a faint thumb on no track
        let sb = &mut child.spacing_mut().scroll;
        *sb = egui::style::ScrollStyle::thin();
        sb.bar_width = 4.0;
        child.visuals_mut().widgets.inactive.bg_fill = rgba(191, 191, 191, 0.25);
        child.visuals_mut().extreme_bg_color = Color32::TRANSPARENT;
        let mut area = egui::ScrollArea::vertical().id_salt(salt).max_height(rect.height()).auto_shrink([false, true]);
        if let Some(off) = scroll {
            area = area.vertical_scroll_offset(off);
        }
        let bg = child.painter().clone();
        let mut shape = None;
        if panel {
            shape = Some(bg.add(egui::Shape::Noop));
        }
        let out = area.show(&mut child, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let inner = Rect::from_min_size(ui.cursor().min + pad, vec2(rect.width() - 2.0 * pad.x, 0.0));
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(inner.min, vec2(inner.width(), f32::INFINITY))).layout(egui::Layout::top_down(egui::Align::Min)));
            c.spacing_mut().item_spacing = vec2(0.0, 0.0);
            content(self, &mut c, inner.width());
            let h = c.min_rect().height();
            ui.allocate_exact_size(vec2(rect.width(), h + 2.0 * pad.y), Sense::hover());
        });
        let shown = out.content_size.y.min(rect.height());
        if let Some(s) = shape {
            bg.set(s, egui::Shape::rect_filled(Rect::from_min_size(rect.min, vec2(rect.width(), shown)), 0.0, PANEL));
        }
        (shown, out.state.clamped_offset().y, out.content_size.y)
    }

    // ------------------------------------------------------------------ documents

    pub(crate) fn paint_doc(&mut self, ui: &mut egui::Ui, body: &DocBody, m: &Metrics, rect: Rect) -> f32 {
        let scroll = self.doc_scroll.take();
        let style = DocStyle { rem: m.rem, ink: INK, dim: INK_DIM, code: C14, bold: m.faces.bold_text };
        let pad = vec2(1.2 * m.rem, 0.9 * m.rem);
        let body = body.clone();
        let (shown, offset, _) = self.region(ui, rect, "doc", true, pad, scroll, &mut |f, ui, w| {
            let _ = w;
            match &body {
                DocBody::Markdown(src) => {
                    let blocks = markdown::parse(src, true);
                    markdown::doc_ui(ui, &blocks, &style);
                }
                DocBody::Fetched => match &f.doc_text {
                    None => {
                        ui.label(egui::RichText::new("Loading…").color(INK_DIM).font(egui::FontId::new(m.rem, m.faces.text())));
                    }
                    Some(Err(why)) => {
                        ui.label(egui::RichText::new(format!("Could not read it here ({why}).")).color(INK_DIM).font(egui::FontId::new(m.rem, m.faces.text())));
                    }
                    Some(Ok(text)) => {
                        let mut job = egui::text::LayoutJob::single_section(text.clone(), Style::new(1.05 * m.rem, m.faces.text(), C14).format());
                        job.wrap.max_width = w;
                        ui.label(job);
                    }
                },
            }
        });
        self.doc_offset = offset;
        self.doc_height = shown;
        shown
    }

    // ------------------------------------------------------------------ What's new

    /// The changelog as a menu's rows, one a release, the cursor's bar and @ on the one it stands
    /// on; its changes under its number and date, as a row's note is under its label.
    pub(crate) fn paint_news(&mut self, ui: &mut egui::Ui, m: &Metrics, rect: Rect, clicks: &mut Clicks) -> f32 {
        let releases = self.releases();
        let cur = self.cursor.clone();
        // a release the cursor lands on glides to the top
        let mut scroll = self.doc_scroll.take();
        let mut tops: Vec<(String, f32, f32)> = Vec::new();
        let (shown, offset, _) = self.region(ui, rect, "news", false, vec2(0.0, 0.0), None, &mut |_f, ui, w| {
            let ctx = ui.ctx().clone();
            let start = ui.cursor().min.y;
            for r in &releases {
                let id = format!("release:{}", r.version);
                let focused = cur.as_deref() == Some(id.as_str());
                let lines = crate::news::release_lines(r);
                let text_w = w - m.label_inset() - m.rem;
                let li_size = m.note();
                let st = DocStyle { rem: li_size, ink: if focused { INK } else { INK_DIM }, dim: INK_DIM, code: C14, bold: m.faces.bold_text };
                let jobs: Vec<_> = lines.iter().map(|l| markdown::job(l, li_size, st.ink, &st, text_w - 1.2 * m.rem).0).map(|j| ctx.fonts_mut(|f| f.layout_job(j))).collect();
                let ver_colour = if focused { GOLD_BRIGHT } else { C7 };
                let ver = galley(&ctx, &r.version, &Style::new(m.action(), m.faces.bold(), ver_colour), f32::INFINITY);
                let when = galley(&ctx, &upper(&r.date), &Style::new(0.8 * m.rem, m.faces.bold(), INK_DIM).spacing(0.14 * 0.8 * m.rem), f32::INFINITY);
                let head_h = ver.size().y.max(when.size().y);
                let body_h: f32 = jobs.iter().map(|g| g.size().y + 0.6 * m.rem).sum();
                let h = 0.7 * m.rem + head_h + 0.3 * m.rem + body_h + 0.7 * m.rem;
                let (rect, resp) = ui.allocate_exact_size(vec2(w, h.max(m.row_height())), Sense::click());
                tops.push((id.clone(), rect.top() - start, rect.height()));
                if resp.clicked() {
                    clicks.chosen = Some((id.clone(), Act::None));
                }
                let p = ui.painter();
                if focused {
                    highlight(p, rect);
                }
                let mut x = rect.left() + m.label_inset();
                let y = rect.top() + 0.7 * m.rem;
                let mark = if focused { "@" } else { "?" };
                let mc = if focused { GOLD_BRIGHT } else { C8 };
                let mg = galley(&ctx, mark, &Style::new(m.action(), m.faces.bold(), mc), f32::INFINITY);
                p.galley(pos2(rect.left() + m.cursor_inset(), y + (ver.size().y - mg.size().y) / 2.0), mg, mc);
                p.galley(pos2(x, y), ver.clone(), ver_colour);
                x += ver.size().x + 0.75 * m.rem;
                let base = y + ver.size().y * 0.78;
                p.galley(pos2(x, base - when.size().y * 0.78), when.clone(), INK_DIM);
                x += when.size().x + 0.75 * m.rem;
                if r.latest || r.fresh {
                    let tag = galley(&ctx, if r.latest { "LATEST" } else { "NEW" }, &Style::new(0.72 * m.rem, m.faces.bold(), Color32::BLACK).spacing(0.14 * 0.72 * m.rem), f32::INFINITY);
                    let tr = Rect::from_min_size(pos2(x, base - tag.size().y * 0.78 - 0.15 * m.rem), tag.size() + vec2(m.rem, 0.3 * m.rem));
                    if r.latest {
                        p.rect_filled(tr, 4.0, GOLD);
                        p.galley(tr.min + vec2(0.5 * m.rem, 0.15 * m.rem), tag, Color32::from_rgb(0x1a, 0x12, 0x04));
                    } else {
                        p.rect_stroke(tr, 4.0, egui::Stroke::new(1.0, GOLD), egui::StrokeKind::Inside);
                        p.galley_with_override_text_color(tr.min + vec2(0.5 * m.rem, 0.15 * m.rem), tag, GOLD_BRIGHT);
                    }
                }
                let mut ly = y + head_h + 0.3 * m.rem;
                for g in &jobs {
                    let lx = rect.left() + m.label_inset();
                    let dash = galley(&ctx, "-", &Style::new(li_size, m.faces.text(), C8), f32::INFINITY);
                    p.galley(pos2(lx, ly), dash, C8);
                    p.galley(pos2(lx + 1.2 * m.rem, ly), g.clone(), st.ink);
                    ly += g.size().y + 0.6 * m.rem;
                }
            }
        });
        // where the cursor's release is: read through a tall one a step at a time, else glide it to the top
        if let Some((_, top, _)) = cur.as_ref().and_then(|c| tops.iter().find(|(i, _, _)| i == c)) {
            if self.news_glide_for.as_deref() != cur.as_deref() {
                self.news_glide_for = cur.clone();
                scroll = Some(if tops.first().is_some_and(|f| Some(&f.0) == cur.as_ref()) { 0.0 } else { *top });
            }
        }
        self.doc_offset = offset;
        self.doc_height = shown;
        self.news_tops = tops;
        if scroll.is_some() {
            self.doc_scroll = scroll;
            ui.ctx().request_repaint();
        }
        shown
    }

    // ------------------------------------------------------------------ the roster

    /// The roster: a table in the console face, the width of the screen, on a dark panel.
    pub(crate) fn paint_roster(&mut self, ui: &mut egui::Ui, m: &Metrics, rect: Rect, clicks: &mut Clicks) -> f32 {
        let entries = self.roster_entries();
        let held = self.roster_held();
        let cur = self.cursor.clone();
        let open = self.details_open.clone();
        let width_css = m.w;
        let cols: Vec<_> = words::ROSTER_COLS.iter().filter(|c| words::roster_col_shown(c, width_css)).copied().collect();
        let ctx = ui.ctx().clone();
        let size = m.rem;
        let head_h = 1.2 * size + 0.7 * m.rem;
        // the columns' widths: what their widest cell needs, the milestone (or the player) the rest
        let cell_w = |c: &words::RosterCol, text: &str| galley(&ctx, text, &m.text(if c.cls == "user" { m.action() } else { size }, C7), f32::INFINITY).size().x;
        let mut widths: Vec<f32> = cols
            .iter()
            .map(|c| {
                let pad = if c.cls == "user" { m.label_inset() + 1.2 * m.rem } else { 1.8 * m.rem };
                let w = entries.iter().map(|e| cell_w(c, &words::roster_cell(e, c.key))).fold(cell_w(c, c.head), f32::max);
                w + pad
            })
            .collect();
        let total: f32 = widths.iter().sum();
        let flex = cols.iter().position(|c| c.cls == "milestone").or_else(|| cols.iter().position(|c| c.cls == "user")).unwrap_or(0);
        let others: f32 = total - widths[flex];
        widths[flex] = (rect.width() - others).max(4.0 * m.rem);
        let scroll = std::mem::take(&mut self.scroll_to_cursor);
        let (shown, _, _) = self.region(ui, rect, "roster", true, vec2(0.0, 0.0), None, &mut |_f, ui, w| {
            let p = ui.painter().clone();
            // the head
            let (hr, _) = ui.allocate_exact_size(vec2(w, head_h), Sense::hover());
            p.rect_filled(hr, 0.0, Color32::from_rgb(8, 7, 12));
            let mut x = hr.left();
            for (c, cw) in cols.iter().zip(&widths) {
                let g = galley(&ctx, c.head, &m.text(size, C8), f32::INFINITY);
                let lx = if c.cls == "user" { x + m.label_inset() } else { x + 0.6 * m.rem };
                p.galley(pos2(lx, hr.center().y - g.size().y / 2.0), g, C8);
                x += cw;
            }
            for e in &entries {
                let id = format!("watch:{}", e.id);
                let focused = cur.as_deref() == Some(id.as_str());
                let (rr, resp) = ui.allocate_exact_size(vec2(w, m.row_height()), Sense::click());
                if resp.clicked() {
                    clicks.chosen = Some((id.clone(), Act::WatchPlayer(e.username.clone())));
                }
                if focused && scroll {
                    ui.scroll_to_rect(rr, None);
                }
                if focused {
                    highlight(&p, rr);
                }
                let idle = e.idle_time.unwrap_or(0) > 0;
                let colour = if focused { GOLD_BRIGHT } else if resp.hovered() { C15 } else if idle { C8 } else { C7 };
                let mut x = rr.left();
                for (c, cw) in cols.iter().zip(&widths) {
                    let text = words::roster_cell(e, c.key);
                    let sz = if c.cls == "user" { m.action() } else { size };
                    let lx = if c.cls == "user" { x + m.label_inset() } else { x + 0.6 * m.rem };
                    let avail = cw - (lx - x) - 0.6 * m.rem;
                    let g = elide(&ctx, &text, &m.text(sz, colour), avail);
                    p.galley(pos2(lx, rr.center().y - g.size().y / 2.0), g, colour);
                    if c.cls == "user" && focused {
                        let at = galley(&ctx, "@", &Style::new(sz, m.faces.bold(), GOLD_BRIGHT), f32::INFINITY);
                        p.galley(pos2(x + m.cursor_inset(), rr.center().y - at.size().y / 2.0), at, GOLD_BRIGHT);
                    }
                    x += cw;
                }
                // the details: the columns the width hides, under the row, when it is opened (right)
                let details = words::roster_details(e, width_css);
                if open.contains(&e.id) && !details.is_empty() {
                    let g = galley(&ctx, &details.join(" · "), &m.text(size, C8), w - m.label_inset() - m.rem);
                    let (dr, dresp) = ui.allocate_exact_size(vec2(w, g.size().y + 0.3 * m.rem), Sense::click());
                    if dresp.clicked() {
                        clicks.chosen = Some((id.clone(), Act::WatchPlayer(e.username.clone())));
                    }
                    p.galley(pos2(dr.left() + m.label_inset(), dr.top()), g, C8);
                }
            }
        });
        let mut used = shown;
        if held > 0 {
            let text = format!("{held} new · press Y to bring them in");
            let g = galley(&ctx, &text, &m.text(m.action(), GOLD_BRIGHT), f32::INFINITY);
            let r = Rect::from_min_size(pos2(rect.left(), rect.top() + shown + 0.4 * m.rem), vec2(g.size().x + m.label_inset() + m.rem, 44.0f32.max(g.size().y + 1.4 * m.rem)));
            let resp = ui.interact(r, egui::Id::NULL.with("roster-held"), Sense::click());
            if resp.hovered() {
                highlight(ui.painter(), r);
                ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(1.0, GOLD), egui::StrokeKind::Inside);
            }
            if resp.clicked() {
                clicks.chosen = Some((self.cursor.clone().unwrap_or_default(), Act::TakeArrivals));
            }
            ui.painter().galley(pos2(r.left() + m.label_inset(), r.center().y - g.size().y / 2.0), g, GOLD_BRIGHT);
            used += 0.4 * m.rem + r.height();
        }
        used
    }

    // ------------------------------------------------------------------ a settings page

    /// One group's settings: a row each, the value between the arrows that turn it, then the
    /// page's own actions; on a dark panel, scrolling in a region of its own.
    pub(crate) fn paint_settings(&mut self, ui: &mut egui::Ui, group: &str, m: &Metrics, rect: Rect, clicks: &mut Clicks) -> f32 {
        let rows = settings::group_rows(group);
        let s = self.settings.clone();
        let cur = self.cursor.clone();
        let at_defaults = settings::group_at_defaults(group, &s);
        let tips_reset = self.gamepad_tips_reset;
        let scroll = std::mem::take(&mut self.scroll_to_cursor);
        let ctx = ui.ctx().clone();
        let (shown, _, _) = self.region(ui, rect, &format!("settings-{group}"), true, vec2(0.0, 0.2 * m.rem), None, &mut |_f, ui, w| {
            let p = ui.painter().clone();
            let narrow = m.narrow;
            let value_w = if narrow { (w * 0.5).min(16.0 * m.rem) } else { (w - m.label_inset() - m.rem - 14.0 * m.rem).clamp(14.0 * m.rem, 24.0 * m.rem) };
            let arrow_w = if m.w <= 520.0 { 2.2 * m.rem } else if narrow { 2.75 * m.rem } else { 44.0 };
            for r in &rows {
                let id = format!("setting:{}", r.key);
                let focused = cur.as_deref() == Some(id.as_str());
                let off = r.is_off(&s);
                let (rr, resp) = ui.allocate_exact_size(vec2(w, m.row_height()), Sense::click());
                if focused && scroll {
                    ui.scroll_to_rect(rr, None);
                }
                if focused {
                    highlight(&p, rr);
                }
                let colour = if off { C8 } else if focused { GOLD_BRIGHT } else if resp.hovered() { INK } else { C7 };
                let label_x = rr.left() + if m.w <= 520.0 { 1.4 * m.rem } else { m.label_inset() };
                let label = galley(&ctx, r.label, &m.text(if m.w <= 520.0 { 1.15 * m.rem } else { m.action() }, colour), f32::INFINITY);
                p.galley(pos2(label_x, rr.center().y - label.size().y / 2.0), label, colour);
                if focused {
                    let at = galley(&ctx, "@", &Style::new(m.action(), m.faces.bold(), GOLD_BRIGHT), f32::INFINITY);
                    let ax = if m.w <= 520.0 { rr.left() + 0.15 * m.rem } else { rr.left() + m.cursor_inset() };
                    p.galley(pos2(ax, rr.center().y - at.size().y / 2.0), at, GOLD_BRIGHT);
                }
                // the value between the angle brackets, which turn it each its own way under the mouse
                // too narrow for a column of values: the value hugs the right, as wide as it reads
                let vsize0 = if m.w <= 520.0 { 0.9 * m.rem } else { m.note() };
                let value_w = if narrow { (galley(&ctx, &r.value(&s), &m.text(vsize0, C7), f32::INFINITY).size().x + 2.0 * arrow_w + 0.4 * m.rem).min(value_w) } else { value_w };
                let right = if narrow { rr.right() } else { rr.right() - m.rem };
                let vr = Rect::from_min_max(pos2(right - value_w, rr.top()), pos2(right, rr.bottom()));
                let vsize = if m.w <= 520.0 { 0.9 * m.rem } else { m.note() };
                let vcol = if off { C8 } else if focused { GOLD_BRIGHT } else if resp.hovered() { INK } else { C7 };
                let val = galley(&ctx, &r.value(&s), &m.text(vsize, vcol), vr.width() - 2.0 * arrow_w);
                p.galley(pos2(vr.center().x - val.size().x / 2.0, vr.center().y - val.size().y / 2.0), val, vcol);
                let mut arrow_hit = false;
                for (d, ch) in [(-1, "<"), (1, ">")] {
                    let ar = if d < 0 { Rect::from_min_size(pos2(vr.left(), vr.center().y - 22.0), vec2(arrow_w, 44.0)) } else { Rect::from_min_size(pos2(vr.right() - arrow_w, vr.center().y - 22.0), vec2(arrow_w, 44.0)) };
                    let ares = ui.interact(ar, egui::Id::NULL.with(("arrow", r.key, d)), Sense::click());
                    let ac = if off { Color32::from_gray(0x44) } else if ares.hovered() || focused { GOLD_BRIGHT } else { INK_DIM };
                    if ares.hovered() && !off {
                        p.rect_stroke(ar, 0.0, egui::Stroke::new(1.0, GOLD), egui::StrokeKind::Inside);
                    }
                    let g = galley(&ctx, ch, &m.text(vsize, ac), f32::INFINITY);
                    p.galley(ar.center() - g.size() / 2.0, g, ac);
                    if ares.clicked() && !off {
                        clicks.turn = Some((r.key.to_string(), d));
                        arrow_hit = true;
                    }
                }
                if resp.clicked() && !arrow_hit && !off {
                    clicks.chosen = Some((id.clone(), Act::Setting(r.key.into())));
                }
            }
            // the page's own actions
            let mut actions: Vec<(String, String, Act, bool)> = Vec::new();
            if group == "Controls" {
                actions.push(("replay-gamepad-tips".into(), if tips_reset { "Gamepad tips reset".into() } else { "Replay gamepad tips".into() }, Act::ReplayTips, false));
                actions.push(("gamepad-controls".into(), "Gamepad controls".into(), Act::Controls, false));
            }
            actions.push(("settings-reset".into(), "Reset to defaults".into(), Act::ResetSettings(group.into()), at_defaults));
            for (i, (id, label, act, off)) in actions.into_iter().enumerate() {
                if i == 0 {
                    ui.add_space(0.3 * m.rem);
                }
                let focused = cur.as_deref() == Some(id.as_str());
                let (rr, resp) = ui.allocate_exact_size(vec2(w, m.row_height()), Sense::click());
                if focused && scroll {
                    ui.scroll_to_rect(rr, None);
                }
                if focused {
                    highlight(&p, rr);
                }
                let colour = if off { C8 } else if focused { GOLD_BRIGHT } else if resp.hovered() { INK } else { C7 };
                let g = galley(&ctx, &label, &m.text(m.action(), colour), f32::INFINITY);
                p.galley(pos2(rr.left() + m.label_inset(), rr.center().y - g.size().y / 2.0), g, colour);
                if focused {
                    let at = galley(&ctx, "@", &Style::new(m.action(), m.faces.bold(), GOLD_BRIGHT), f32::INFINITY);
                    p.galley(pos2(rr.left() + m.cursor_inset(), rr.center().y - at.size().y / 2.0), at, GOLD_BRIGHT);
                }
                if resp.clicked() {
                    clicks.chosen = Some((id, act));
                }
            }
        });
        shown
    }

    // ------------------------------------------------------------------ the gamepad sheet

    /// What every button on the pad does, as two tables side by side, one per half of the pad.
    pub(crate) fn paint_controls(&mut self, ui: &mut egui::Ui, m: &Metrics, rect: Rect) -> f32 {
        let (shown, _, _) = self.region(ui, rect, "controls", true, vec2(1.2 * m.rem, 0.9 * m.rem), None, &mut |_f, ui, w| {
            let ctx = ui.ctx().clone();
            let p = ui.painter().clone();
            let two = w > 50.0 * m.rem;
            let col_w = if two { (w - 2.0 * m.rem) / 2.0 } else { w };
            let start = ui.cursor().min;
            let mut bottom: f32 = start.y;
            for (k, side) in CONTROLS_SHEET.iter().enumerate() {
                let x0 = if two { start.x + k as f32 * (col_w + 2.0 * m.rem) } else { start.x };
                let mut y = if two { start.y } else { bottom + if k > 0 { m.rem } else { 0.0 } };
                for (button, tap, hold) in side.iter() {
                    let key = galley(&ctx, button, &Style::new(m.rem, m.faces.bold(), GOLD_BRIGHT), f32::INFINITY);
                    let text = if hold.is_empty() { tap.to_string() } else { format!("{tap}   hold: {hold}") };
                    let g = galley(&ctx, &text, &m.text(m.rem, INK), col_w - 7.0 * m.rem);
                    p.galley(pos2(x0, y), key, GOLD_BRIGHT);
                    p.galley(pos2(x0 + 7.0 * m.rem, y), g.clone(), INK);
                    y += g.size().y.max(m.rem * 1.4) + 0.5 * m.rem;
                }
                bottom = bottom.max(y);
            }
            let note = galley(&ctx, CONTROLS_NOTE, &m.text(0.92 * m.rem, INK_DIM).line(1.5 * 0.92 * m.rem), w);
            p.galley(pos2(start.x, bottom + 0.6 * m.rem), note.clone(), INK_DIM);
            ui.allocate_exact_size(vec2(w, bottom - start.y + 0.6 * m.rem + note.size().y), Sense::hover());
        });
        shown
    }

    // ------------------------------------------------------------------ the rc file

    /// The rc file: a box of text as wide as the rows under it.
    pub(crate) fn paint_rc_box(&mut self, ui: &mut egui::Ui, r: &Row, rect: Rect, m: &Metrics, clicks: &mut Clicks) {
        let focused = self.current() == Some(r.id.as_str());
        let box_rect = Rect::from_min_max(pos2(rect.left(), rect.top() + 0.4 * m.rem), pos2(rect.right(), rect.bottom() - 0.6 * m.rem));
        ui.painter().rect_filled(box_rect, 0.0, PANEL);
        let Some(rc) = &mut self.rc else { return };
        let ready = rc.state == crate::words::RcState::Ready;
        let editing = rc.editing && ready;
        let colour = if ready { C15 } else { INK_DIM };
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(box_rect.shrink2(vec2(m.rem, 0.8 * m.rem))));
        let font = egui::FontId::new(m.note(), m.faces.text());
        let mut out = None;
        egui::ScrollArea::both().id_salt("rc-scroll").max_height(box_rect.height() - 1.6 * m.rem).auto_shrink([false, false]).show(&mut child, |ui| {
            let edit = egui::TextEdit::multiline(&mut rc.text).id(egui::Id::NULL.with("rc-text")).font(font.clone()).text_color(colour).frame(egui::Frame::NONE).code_editor().desired_width(f32::INFINITY).interactive(ready).lock_focus(true);
            out = Some(ui.add(edit));
        });
        if let Some(resp) = out {
            if resp.clicked() || resp.gained_focus() {
                clicks.chosen = Some((r.id.clone(), Act::None));
                rc.editing = ready;
            }
            if editing && !resp.has_focus() {
                resp.request_focus();
            }
            if !editing && resp.has_focus() {
                resp.surrender_focus();
            }
        }
        if focused || editing {
            ui.painter().rect_stroke(box_rect, 0.0, egui::Stroke::new(1.0, GOLD), egui::StrokeKind::Inside);
            self.cursor_rect = Some(rect);
        }
    }
}

/// Text cut short with an ellipsis to fit `width`.
fn elide(ctx: &egui::Context, text: &str, style: &Style, width: f32) -> std::sync::Arc<egui::Galley> {
    let g = galley(ctx, text, style, f32::INFINITY);
    if g.size().x <= width || text.is_empty() {
        return g;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 0 {
        n -= 1;
        let t: String = chars[..n].iter().collect::<String>() + "…";
        let g = galley(ctx, &t, style, f32::INFINITY);
        if g.size().x <= width {
            return g;
        }
    }
    galley(ctx, "…", style, f32::INFINITY)
}

/// The gamepad sheet (controls-sheet.ts over bindings.ts `controlSheet`): each half of the pad, top
/// to bottom: the button, what a tap does, and what a hold does.
pub const CONTROLS_SHEET: [&[(&str, &str, &str)]; 2] = [
    &[
        ("LT", "Explore", "Explore while held"),
        ("LB", "Actions tab", ""),
        ("L stick", "Move one step the way it points on the screen", ""),
        ("L3", "Hotbar", ""),
        ("D-pad", "Move one step the way it points on the screen", ""),
        ("Select", "Level map", ""),
    ],
    &[
        ("RT", "Autofight", ""),
        ("RB", "Commands", ""),
        ("R stick", "Left / right turn the view; up / down zoom", ""),
        ("R3", "Reset view", ""),
        ("A", "Interact", ""),
        ("B", "Examine", ""),
        ("X", "Actions", ""),
        ("Y", "Equipment", ""),
        ("Start", "Isorun menu", ""),
    ],
];

pub const CONTROLS_NOTE: &str = "X opens actions as tabs (Spells, Abilities, Evocables, Quiver, Memorise); LB / RB turn them, Y swaps weapons and LT shouts. Select opens the level map, where LB / RB find stairs, Y travels to a branch, L3 finds items and R3 opens the overview. Y opens the inventory: with the HUD on Dock the paper doll and the backpack, where A lists what can be done with an item, Y does the first and X describes it; on Classic the pack, whose pages LB / RB turn. L3 puts the cursor on the dock: the quick slots, the actions and a button for every screen; A uses, Y changes a slot, B goes back. Start opens the Isorun menu, with your character on one tab and the game options on the other; LB / RB or left / right switches its tabs. Commands already on gamepad buttons are omitted. Up / down chooses a command; Page Up / Down pages. A / Start selects, B backs out, Select closes the menu outright. Browsing sends no game commands.";
