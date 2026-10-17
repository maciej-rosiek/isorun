//! Character creation (overlays.ts `renderNewgame`): the server's `OuterMenu` grids (species,
//! background, weapon) as buttons, the description and the hero of the button under the cursor
//! beside them. The cursor is ours and moves at once; each move tells the server with
//! `outer_menu_focus`, as the official client does when the mouse enters a button, and a
//! `button_focus` the server moved itself re-seats it.

use crate::focus::{Act, Focusable};
use crate::frame::{self, Screen};
use crate::rich::{self, TextStyle};
use crate::tiles::{self, Tile};
use crate::{OverlayCx, Overlays};
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Vec2};
use isorun_ui::widgets::{self, hex, palette, rgba, PanelLook};
use isorun_webtiles::{cm, formatted_string_to_text, Popup};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Button {
    pub hotkey: i64,
    pub x: i32,
    pub y: i32,
    pub tiles: Vec<Tile>,
    pub key: Option<String>,
    pub name: String,
    /// the name is a formatted string of the server's (no `k - name` to split)
    pub formatted: bool,
    pub suffixes: Vec<String>,
    pub description: String,
    pub highlight: i64,
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub struct Grid {
    pub menu_id: String,
    pub buttons: Vec<Button>,
    pub labels: Vec<(i32, i32, String)>,
    pub cols: i32,
    pub rows: i32,
}

fn grid(data: Option<&Value>) -> Grid {
    let Some(d) = data else { return Grid::default() };
    let mut g = Grid { menu_id: d.get("menu_id").map(isorun_webtiles::js_string).unwrap_or_default(), cols: 1, rows: 0, ..Default::default() };
    for b in d.get("buttons").and_then(|v| v.as_array()).into_iter().flatten() {
        let labels: Vec<String> = match b.get("labels").and_then(|v| v.as_array()) {
            Some(a) => a.iter().map(isorun_webtiles::js_string).collect(),
            None => vec![b.get("label").map(isorun_webtiles::js_string).unwrap_or_default()],
        };
        let lead = labels.first().cloned().unwrap_or_default();
        let plain = formatted_string_to_text(&lead);
        let split = plain.trim_start().split_once(" - ").filter(|(k, _)| !k.contains(char::is_whitespace));
        let (key, name, formatted) = match split {
            Some((k, n)) => (Some(k.to_string()), n.to_string(), false),
            None => (None, lead.clone(), true),
        };
        let x = b.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let y = b.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        g.cols = g.cols.max(x + 1);
        g.rows = g.rows.max(y + 1);
        let hotkey = b.get("hotkey").and_then(|v| v.as_i64()).unwrap_or(0);
        let label = labels.iter().map(|l| formatted_string_to_text(l)).collect::<Vec<_>>().join(" ").trim().to_string();
        g.buttons.push(Button {
            hotkey,
            x,
            y,
            tiles: Tile::list(b.get("tile")),
            key,
            name,
            formatted,
            suffixes: labels[1..].to_vec(),
            description: b.get("description").map(isorun_webtiles::js_string).unwrap_or_default(),
            highlight: b.get("highlight_colour").and_then(|v| v.as_i64()).unwrap_or(0),
            label: if label.is_empty() { char::from_u32(hotkey as u32).map(String::from).unwrap_or_default() } else { label },
        });
    }
    for l in d.get("labels").and_then(|v| v.as_array()).into_iter().flatten() {
        let x = l.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let y = l.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        g.rows = g.rows.max(y + 1);
        g.labels.push((x, y, l.get("label").map(isorun_webtiles::js_string).unwrap_or_default()));
    }
    g
}

/// The main grid and the sub grid (the random and recommended rows).
pub fn grids(p: &Popup) -> (Grid, Grid) {
    (grid(p.data.get("main-items")), grid(p.data.get("sub-items")))
}

/// The cursor's stops, the screen, where the cursor starts, the server's focus and whether the
/// server's focus was an echo of ours.
pub fn focus(p: &Popup) -> (String, Vec<Focusable>, i32, String, bool) {
    let (main, sub) = grids(p);
    let focus_key = p.state.get("button_focus").and_then(|v| v.as_i64());
    let mut items = Vec::new();
    // the sub grid's rows follow the main grid's, so down from the last species reaches the random row
    let mut base = 0;
    for (g, cls) in [(&main, "main"), (&sub, "sub")] {
        for b in &g.buttons {
            let focus_msg = cm::outer_menu_focus(b.hotkey, &g.menu_id);
            items.push(Focusable {
                label: b.label.clone(),
                act: Act::send([focus_msg.clone(), cm::key(b.hotkey as i32)]),
                on_focus: vec![focus_msg],
                row: Some(base + b.y),
                col: Some(b.x),
                group: cls.into(),
                id: Some(format!("{cls}:{}", b.hotkey)),
                ..Default::default()
            });
        }
        base += g.rows;
    }
    let all: Vec<&Button> = main.buttons.iter().chain(sub.buttons.iter()).collect();
    let sel = focus_key.and_then(|k| all.iter().position(|b| b.hotkey == k));
    let screen = format!("popup:newgame-choice:{}", main.menu_id);
    let server_focus = format!("{screen}:{}", focus_key.map(|k| k.to_string()).unwrap_or_default());
    let from_client = p.state.get("from_client").is_some_and(isorun_webtiles::truthy);
    (screen, items, sel.unwrap_or(0) as i32, server_focus, from_client)
}

struct ButtonLay {
    rect: Rect,
    id: String,
    index: usize,
    sub: bool,
}

impl Overlays {
    pub(crate) fn draw_newgame(&mut self, ctx: &egui::Context, cx: &mut OverlayCx, p: &Popup, top: bool, screen: &Screen) {
        let compact = screen.w() <= 900.0 || screen.compact();
        let u = if screen.compact() { 0.7 * screen.scale } else { screen.hu() };
        let font = 17.0 * u;
        let (main, sub) = grids(p);
        let lit = if top { self.nav.current().map(|f| f.key().to_string()) } else { None };
        let server_sel = p.state.get("button_focus").and_then(|v| v.as_i64());
        let shown_id = lit.clone().or_else(|| server_sel.map(|k| format!("main:{k}")));
        // the frame
        let width = (0.96 * screen.w()).min(1500.0 * u).floor();
        let width = if screen.compact() { screen.w() - 12.0 } else { width };
        let max_h = if screen.compact() { screen.h() - 12.0 } else { 0.94 * screen.h() };
        let title_font = FontId::new(34.0 * u, isorun_ui::fonts::caps());
        let title = spaced(ctx, &formatted_string_to_text(&p.data.get("title").map(isorun_webtiles::js_string).unwrap_or_default()), title_font, 0.08 * 34.0 * u, palette::GOLD_BRIGHT);
        let doll = tiles::doll(p.data.get("doll"));
        let head_h = (16.0 * u + (34.0 * u * 1.1).max(if doll.is_empty() { 0.0 } else { 96.0 }) + 4.0 * u).ceil();
        let prompt = p.data.get("prompt").map(isorun_webtiles::js_string).filter(|s| !s.is_empty());
        let prompt_st = TextStyle::new(ctx, 18.0 * u, palette::BRONZE_BRIGHT).line(18.0 * u * 1.3);
        let prompt_g = prompt.as_ref().map(|t| rich::galley(ctx, rich::formatted(t, &prompt_st, width - 48.0 * u)));
        let prompt_h = prompt_g.as_ref().map_or(0.0, |g| g.size().y + 14.0 * u + 1.0);
        // the details column: the heroes and the descriptions, each as tall as its tallest pane
        let details_w = if compact { width - 2.0 } else { 420.0 * u };
        let desc_st = TextStyle::new(ctx, font, hex(0xcfc4ad)).line(1.5 * font);
        let all: Vec<&Button> = main.buttons.iter().chain(sub.buttons.iter()).collect();
        let desc_gs: Vec<_> = all.iter().map(|b| rich::galley(ctx, rich::formatted(&b.description, &desc_st, details_w - 48.0 * u))).collect();
        let desc_h = desc_gs.iter().map(|g| g.size().y).fold(0.0, f32::max) + 28.0 * u;
        let hero_font = FontId::new(30.0 * u, isorun_ui::fonts::caps());
        let hero_name = |b: &Button| if b.formatted { formatted_string_to_text(&b.name).trim().to_string() } else { b.name.clone() };
        let hero_h = if compact { 0.0 } else { 24.0 * u + 128.0 + 8.0 * u + 30.0 * u * 1.1 + 12.0 * u };
        // the grids: each column as wide as its widest button, the room left shared out
        let key_font = 0.85 * font;
        let lay_grid = |g: &Grid, font: f32, min_h: f32, inner_w: f32| -> (Vec<f32>, Vec<f32>, Vec<Rect>, f32) {
            let st = TextStyle::new(ctx, font, palette::INK).bold();
            let gap_x = 12.0 * u;
            let gap_y = 6.0 * u;
            let mut col_w = vec![0.0f32; g.cols.max(1) as usize];
            let mut row_h = vec![0.0f32; g.rows.max(1) as usize];
            let mut sizes = Vec::new();
            for b in &g.buttons {
                let name_w = rich::natural_width(ctx, if b.formatted { rich::formatted(&b.name, &st, 0.0) } else { rich::plain(&b.name, &st, 0.0) });
                let key_w = b.key.as_ref().map_or(0.0, |k| (rich::natural_width(ctx, rich::plain(k, &TextStyle::new(ctx, font * 0.85, palette::BRONZE_BRIGHT), 0.0)) + 0.6 * key_font).max(1.5 * key_font) + 2.0 + 10.0 * u);
                let suf_w: f32 = b.suffixes.iter().map(|s| rich::natural_width(ctx, rich::formatted(s, &TextStyle::new(ctx, font, TERM8), 0.0)) + 10.0 * u).sum();
                let tile_w = if b.tiles.is_empty() { 0.0 } else { 48.0 + 10.0 * u };
                let w = 6.0 * u + tile_w + key_w + name_w + suf_w + 12.0 * u + 2.0;
                let h = (if b.tiles.is_empty() { widgets::normal_line_height(font) } else { 48.0 } + 8.0 * u + 2.0).max(min_h);
                col_w[b.x as usize] = col_w[b.x as usize].max(w);
                row_h[b.y as usize] = row_h[b.y as usize].max(h);
                sizes.push(h);
            }
            for (x, y, l) in &g.labels {
                let lw = rich::natural_width(ctx, rich::plain(&formatted_string_to_text(l), &TextStyle::new(ctx, 20.0 * u, palette::GOLD), 0.0)) + 0.1 * 20.0 * u * l.len() as f32;
                col_w[*x as usize] = col_w[*x as usize].max(lw);
                row_h[*y as usize] = row_h[*y as usize].max(10.0 * u + 20.0 * u * 1.2 + 4.0 * u + 1.0);
            }
            let used: f32 = col_w.iter().sum::<f32>() + gap_x * (col_w.len() as f32 - 1.0);
            let free = (inner_w - used).max(0.0);
            let share = free / col_w.len() as f32;
            for w in col_w.iter_mut() {
                *w += share;
            }
            let h: f32 = row_h.iter().sum::<f32>() + gap_y * (row_h.len() as f32 - 1.0).max(0.0) + 28.0 * u;
            let _ = sizes;
            (col_w, row_h, Vec::new(), h)
        };
        let choices_w = if compact { width - 2.0 } else { width - 2.0 - details_w - 1.0 };
        let grid_inner = choices_w - 40.0 * u;
        let (main_cols, main_rows, _, main_h) = lay_grid(&main, font, 56.0 * u, grid_inner);
        let (sub_cols, sub_rows, _, sub_h) = if sub.buttons.is_empty() { (vec![], vec![], vec![], 0.0) } else { lay_grid(&sub, 0.9 * font, 36.0 * u, grid_inner) };
        let choices_h = main_h + if sub.buttons.is_empty() { 0.0 } else { sub_h + 1.0 };
        let details_h = hero_h + desc_h;
        let cols_h_natural = if compact { choices_h + 1.0 + details_h.min(0.24 * screen.h()) } else { choices_h.max(details_h) };
        let height = (head_h + prompt_h + cols_h_natural + 2.0).min(max_h).ceil();
        let cols_h = height - 2.0 - head_h - prompt_h;
        let rect = Rect::from_center_size(screen.rect.center(), Vec2::new(width, height));
        let rect = Rect::from_min_size(Pos2::new(rect.min.x.round(), rect.min.y.round()), rect.size());
        let key = p.stack_order;
        let scroll = self.scroll.get(&key).copied().unwrap_or(0.0);
        let mut new_scroll = scroll;
        let mut hovered_id: Option<String> = None;
        let mut clicked_id: Option<String> = None;
        let pointer_live = self.pointer_live && self.pointer_moved;
        frame::layer(ctx, egui::Id::unique(("isorun-newgame", key)), rect, |ui, _| {
            let pt = ui.painter().clone();
            widgets::paint_panel(&pt, rect, &PanelLook::newgame(u));
            // faint lines across the parchment
            let stripes = pt.with_clip_rect(rect.shrink(1.0));
            let mut sy = rect.top() + 1.0;
            while sy < rect.bottom() {
                stripes.rect_filled(Rect::from_min_size(Pos2::new(rect.left(), sy), Vec2::new(width, 2.0)), 0.0, rgba(255, 255, 255, 0.012));
                sy += 6.0;
            }
            // the title, centred, the doll before it
            let head = Rect::from_min_size(rect.min + Vec2::splat(1.0), Vec2::new(width - 2.0, head_h));
            widgets::paint_texture(&pt, head, 0.0, widgets::linear_gradient(ui.ctx(), 180, &[(0.0, rgba(60, 45, 20, 0.5)), (1.0, rgba(60, 45, 20, 0.0))]));
            let content_w = title.size().x + if doll.is_empty() { 0.0 } else { 96.0 + 0.6 * 34.0 * u };
            let mut x = head.center().x - content_w / 2.0;
            let cy = head.top() + 16.0 * u + (head_h - 20.0 * u) / 2.0;
            let title_y = cy - title.size().y / 2.0;
            if !doll.is_empty() {
                tiles::paint(cx.tiles, &pt, &doll, tiles::square(x, cy, 96.0));
                x += 96.0 + 0.6 * 34.0 * u;
            }
            let shadow = Vec2::new(0.0, 2.0);
            pt.galley_with_override_text_color(Pos2::new(x, cy - title.size().y / 2.0) + shadow, title.clone(), rgba(0, 0, 0, 0.8));
            widgets::galley_bold(&pt, Pos2::new(x, title_y), title.clone(), palette::GOLD_BRIGHT);
            let mut y = head.bottom();
            if let Some(g) = &prompt_g {
                pt.galley(Pos2::new(rect.left() + 1.0 + 24.0 * u, y), g.clone(), palette::BRONZE_BRIGHT);
                y += g.size().y + 14.0 * u;
                widgets::rule(&pt, rect.left() + 1.0..=rect.right() - 1.0, y, palette::RULE);
                y += 1.0;
            }
            // the choices, scrolling when the screen is short
            let choices = Rect::from_min_size(Pos2::new(rect.left() + 1.0, y), Vec2::new(choices_w, if compact { (cols_h - details_h.min(0.24 * screen.h()) - 1.0).max(40.0) } else { cols_h }));
            new_scroll = frame::wheel(ui, choices, choices_h, scroll);
            let cp = pt.with_clip_rect(choices);
            let mut lays: Vec<ButtonLay> = Vec::new();
            let mut gy = choices.top() - new_scroll;
            let mut index = 0;
            for (g, cols, rows, gfont, sub_grid) in [(&main, &main_cols, &main_rows, font, false), (&sub, &sub_cols, &sub_rows, 0.9 * font, true)] {
                if g.buttons.is_empty() {
                    continue;
                }
                if sub_grid {
                    widgets::rule(&cp, choices.left()..=choices.right(), gy, palette::RULE);
                    gy += 1.0;
                }
                let gx = choices.left() + 20.0 * u;
                let top_y = gy + 14.0 * u;
                let col_x = |c: i32| gx + cols[..c as usize].iter().sum::<f32>() + 12.0 * u * c as f32;
                let row_y = |r: i32| top_y + rows[..r as usize].iter().sum::<f32>() + 6.0 * u * r as f32;
                for (lx, ly, l) in &g.labels {
                    let r = Rect::from_min_size(Pos2::new(col_x(*lx), row_y(*ly)), Vec2::new(cols[*lx as usize], rows[*ly as usize]));
                    let lg = spaced(ui.ctx(), &formatted_string_to_text(l), FontId::new(20.0 * u, isorun_ui::fonts::caps()), 0.1 * 20.0 * u, palette::GOLD);
                    widgets::galley_bold(&cp, Pos2::new(r.left(), r.top() + 10.0 * u + (20.0 * u * 1.2 - lg.size().y) / 2.0), lg, palette::GOLD);
                    widgets::rule(&cp, r.left()..=r.right(), r.bottom() - 1.0, palette::RULE);
                }
                for b in &g.buttons {
                    let id = format!("{}:{}", if sub_grid { "sub" } else { "main" }, b.hotkey);
                    let r = Rect::from_min_size(Pos2::new(col_x(b.x), row_y(b.y)), Vec2::new(cols[b.x as usize], rows[b.y as usize]));
                    let on = shown_id.as_deref() == Some(id.as_str());
                    draw_button(ui, &cp, cx.tiles, b, r, on, u, gfont);
                    lays.push(ButtonLay { rect: r, id, index, sub: sub_grid });
                    index += 1;
                }
                gy += rows.iter().sum::<f32>() + 6.0 * u * (rows.len() as f32 - 1.0).max(0.0) + 28.0 * u;
            }
            for l in &lays {
                if !choices.contains(l.rect.center()) {
                    continue;
                }
                let resp = ui.interact(l.rect, egui::Id::unique(("isorun-ng", &l.id)), egui::Sense::click());
                if resp.hovered() && pointer_live {
                    hovered_id = Some(l.id.clone());
                }
                if resp.hovered() && shown_id.as_deref() != Some(l.id.as_str()) {
                    widgets::outline(&cp, l.rect.shrink(1.0), 4.0 * u, palette::BRONZE);
                }
                if resp.clicked() {
                    clicked_id = Some(l.id.clone());
                }
                let _ = (l.index, l.sub);
            }
            // the details: the hero of the button under the cursor, and its description
            let details = if compact {
                Rect::from_min_max(Pos2::new(rect.left() + 1.0, choices.bottom() + 1.0), Pos2::new(rect.right() - 1.0, rect.bottom() - 1.0))
            } else {
                Rect::from_min_size(Pos2::new(choices.right() + 1.0, y), Vec2::new(details_w, cols_h))
            };
            if compact {
                widgets::rule(&pt, details.left()..=details.right(), details.top() - 1.0, palette::RULE);
            } else {
                pt.rect_filled(Rect::from_min_size(Pos2::new(details.left() - 1.0, details.top()), Vec2::new(1.0, details.height())), 0.0, palette::RULE);
            }
            widgets::paint_texture(&pt, details, 0.0, widgets::radial_gradient(ui.ctx(), (0.5, 0.2), details.width() / details.height().max(1.0), &[(0.0, rgba(120, 90, 40, 0.16)), (0.7, rgba(120, 90, 40, 0.0))]));
            let dp = pt.with_clip_rect(details);
            let shown = shown_id.as_ref().and_then(|id| lays.iter().find(|l| &l.id == id)).map(|l| l.index);
            if let Some(i) = shown {
                let b = all[i];
                let mut dy = details.top();
                if !compact {
                    dy += 24.0 * u;
                    if !b.tiles.is_empty() {
                        let sq = Rect::from_min_size(Pos2::new(details.center().x - 64.0, dy), Vec2::splat(128.0));
                        tiles::paint(cx.tiles, &dp, &b.tiles, sq);
                    }
                    dy += 128.0 + 8.0 * u;
                    let ng = spaced(ui.ctx(), &hero_name(b).to_uppercase(), hero_font.clone(), 0.06 * 30.0 * u, palette::GOLD_BRIGHT);
                    let at = Pos2::new(details.center().x - ng.size().x / 2.0, dy);
                    dp.galley_with_override_text_color(at + Vec2::new(0.0, 2.0), ng.clone(), rgba(0, 0, 0, 0.8));
                    widgets::galley_bold(&dp, at - Vec2::new(0.0, 3.0 * u), ng, palette::GOLD_BRIGHT);
                    dy = details.top() + hero_h;
                }
                dp.galley(Pos2::new(details.left() + 24.0 * u, dy + 8.0 * u), desc_gs[i].clone(), hex(0xcfc4ad));
            }
        });
        self.scroll.insert(key, new_scroll);
        if !top {
            return;
        }
        let index_of = |id: &str| self.nav.items().iter().position(|f| f.key() == id).map(|i| i as i32);
        if let Some(id) = clicked_id {
            if let Some(i) = index_of(&id) {
                self.nav.focus(i);
                self.nav.take_fired();
                self.nav.activate();
                self.run_fired(cx);
            }
        } else if let Some(id) = hovered_id {
            // the pointer moves the one cursor too, so A and Enter take what the mouse is over
            if lit.as_deref() != Some(id.as_str()) {
                if let Some(i) = index_of(&id) {
                    self.nav.focus(i);
                    self.run_fired(cx);
                }
            }
        }
    }
}

const TERM8: Color32 = Color32::from_rgb(0x55, 0x57, 0x53);

/// Text in Cinzel's capitals with CSS letter-spacing.
fn spaced(ctx: &egui::Context, text: &str, font: FontId, spacing: f32, colour: Color32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    for (i, c) in text.chars().enumerate() {
        job.append(&c.to_string(), if i == 0 { 0.0 } else { spacing }, egui::TextFormat { font_id: font.clone(), color: colour, ..Default::default() });
    }
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// One button of the grid (`.newgame .button`).
#[allow(clippy::too_many_arguments)]
fn draw_button(_ui: &egui::Ui, p: &egui::Painter, icons: &mut isorun_ui::tiles::TileIcons, b: &Button, r: Rect, on: bool, u: f32, font: f32) {
    let ctx = p.ctx().clone();
    let dim = b.highlight == 7 && !on;
    let fade = |c: Color32| if dim { c.gamma_multiply(0.5) } else { c };
    let r = if on { r.translate(Vec2::new(2.0 * u, 0.0)) } else { r };
    let radius = 4.0 * u;
    if on {
        p.add(egui::epaint::Shadow { offset: [0, 0], blur: (14.0 * u) as u8, spread: 0, color: rgba(232, 201, 138, 0.35) }.as_shape(r, egui::CornerRadius::same(radius as u8)));
        widgets::paint_texture(p, r, radius, widgets::linear_gradient(&ctx, 180, &[(0.0, rgba(120, 90, 40, 0.55)), (1.0, rgba(60, 45, 20, 0.55))]));
        p.rect_stroke(r.shrink(1.0), radius, egui::Stroke::new(1.0, rgba(255, 230, 170, 0.25)), egui::StrokeKind::Inside);
        p.rect_stroke(r, radius, egui::Stroke::new(1.0, palette::GOLD_BRIGHT), egui::StrokeKind::Inside);
    } else {
        widgets::paint_texture(p, r, radius, widgets::linear_gradient(&ctx, 180, &[(0.0, fade(palette::IRON_RAISED)), (1.0, fade(palette::IRON))]));
        p.rect_stroke(r, radius, egui::Stroke::new(1.0, fade(palette::IRON_LINE)), egui::StrokeKind::Inside);
        p.rect_filled(Rect::from_min_size(r.min + Vec2::new(radius, 1.0), Vec2::new(r.width() - 2.0 * radius, 1.0)), 0.0, rgba(255, 255, 255, 0.04));
    }
    if b.highlight == 2 {
        // a recommended choice: a green edge on the left
        p.rect_filled(Rect::from_min_size(r.min, Vec2::new(3.0 * u, r.height())), egui::CornerRadius { nw: radius as u8, sw: radius as u8, ne: 0, se: 0 }, hex(0x5f9e3f));
    }
    let ink = if on { palette::GOLD_BRIGHT } else { fade(palette::INK) };
    let mut x = r.left() + 1.0 + 6.0 * u;
    let cy = r.center().y;
    if !b.tiles.is_empty() {
        let sq = tiles::square(x, cy, 48.0);
        if dim {
            // `opacity: 0.5` on the whole button: the tile fades with it
            let tp = p.clone();
            tiles::paint(icons, &tp, &b.tiles, sq);
            p.rect_filled(sq, 0.0, rgba(18, 17, 16, 0.5));
        } else {
            tiles::paint(icons, p, &b.tiles, sq);
        }
        x += 48.0 + 10.0 * u;
    }
    if let Some(k) = &b.key {
        let kst = TextStyle::new(&ctx, 0.85 * font, if on { palette::BRONZE_BRIGHT } else { fade(palette::BRONZE_BRIGHT) });
        let g = rich::galley(&ctx, rich::plain(k, &kst, 0.0));
        let w = (g.size().x + 0.6 * 0.85 * font).max(1.5 * 0.85 * font) + 2.0;
        let kr = Rect::from_center_size(Pos2::new(x + w / 2.0, cy), Vec2::new(w, g.size().y + 2.0));
        p.rect_filled(kr, 3.0 * u, fade(palette::IRON_DEEP));
        p.rect_stroke(kr, 3.0 * u, egui::Stroke::new(1.0, fade(palette::BRONZE_DIM)), egui::StrokeKind::Inside);
        p.galley(kr.center() - g.size() / 2.0, g, kst.colour);
        x += w + 10.0 * u;
    }
    let st = TextStyle::new(&ctx, font, ink);
    let st = if b.highlight == 7 { TextStyle { family: FontFamily::Monospace, ..st } } else { st.bold() };
    let ng = rich::galley(&ctx, if b.formatted { rich::formatted(&b.name, &st, 0.0) } else { rich::plain(&b.name, &st, 0.0) });
    p.galley(Pos2::new(x, cy - ng.size().y / 2.0), ng, ink);
    let mut rx = r.right() - 1.0 - 12.0 * u;
    for s in b.suffixes.iter().rev() {
        let g = rich::galley(&ctx, rich::formatted(s, &TextStyle::new(&ctx, font, fade(TERM8)), 0.0));
        rx -= g.size().x;
        p.galley(Pos2::new(rx, cy - g.size().y / 2.0), g, TERM8);
        rx -= 10.0 * u;
    }
}

#[allow(dead_code)]
fn unused(_f: FontFamily) {}
