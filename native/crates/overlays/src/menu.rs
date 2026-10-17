//! crawl's menus (overlays.ts `renderMenu` and the menu navigation): the title (or the filter's
//! field), the rows with their tiles, the `more` line and its switches, the hover the cursor moves
//! and tells the server about (`menu_hover`, `menu_scroll`), and the paging menu.js does on the
//! client.

use crate::frame::{self, Screen};
use crate::menu_nav::{self, MenuNavKey, Visible};
use crate::rich::{self, TextStyle};
use crate::tiles::{self, Tile};
use crate::{MenuGeom, MoreSwitchAt, OverlayCx, Overlays, GAME_MENU_TAG};
use egui::{Color32, Pos2, Rect, Vec2};
use isorun_ui::widgets::{self, hex, rgba};
use isorun_webtiles::{cm, formatted_string_to_text, Keys, MenuFlag, MenuState};

/// menu.js `item_selectable`, or the pad's: a level-2 row of an arrows-select menu that prints no hotkey.
pub fn pad_hoverable(menu: &MenuState, i: i64) -> bool {
    menu.items.get(i as usize).and_then(|x| x.as_ref()).is_some_and(|it| it.level.unwrap_or(2) == 2) && menu.flags & MenuFlag::ARROWS_SELECT != 0
}

pub fn hoverable(menu: &MenuState) -> Vec<i64> {
    (0..menu.items.len() as i64).filter(|&i| menu_nav::item_selectable(menu, i) || pad_hoverable(menu, i)).collect()
}

/// The more line as shown: `more` when the list overflows, else `alt_more`, `XXX` the scroll position.
fn more_text(menu: &MenuState, overflow: bool, scroll: f32, max: f32) -> String {
    let shown = if overflow { &menu.more } else { &menu.alt_more };
    let pct = if scroll <= 0.0 || max <= 0.0 {
        "top".to_string()
    } else if scroll >= max {
        "bot".to_string()
    } else {
        let p = format!("{:.0}%", scroll * 100.0 / max);
        if p.len() == 2 {
            format!(" {p}")
        } else {
            p
        }
    };
    shown.replacen("XXX", &pct, 1)
}

/// The switches of a more line, with where each stands in it.
pub fn switches(plain: &str) -> Vec<MoreSwitchAt> {
    let chars: Vec<char> = plain.chars().collect();
    menu_nav::parse_more_switches(plain)
        .into_iter()
        .map(|sw| {
            let line_start = chars[..sw.start].iter().rposition(|&c| c == '\n').map_or(0, |p| p + 1);
            let line = chars[..line_start].iter().filter(|&&c| c == '\n').count();
            MoreSwitchAt { key: sw.key, label: sw.label, line, col: sw.start - line_start, len: sw.end - sw.start }
        })
        .collect()
}

/// The rect of characters `[col, col + len)` of line `line` of a galley laid out from text with `\n`s.
pub fn char_rect(galley: &egui::Galley, text: &str, line: usize, col: usize, len: usize) -> Option<Rect> {
    let mut idx = 0;
    for (n, l) in text.split('\n').enumerate() {
        if n == line {
            let a = galley.pos_from_cursor(egui::text::CCursor::new(idx + col));
            let b = galley.pos_from_cursor(egui::text::CCursor::new(idx + col + len));
            return Some(Rect::from_min_max(a.min, Pos2::new(b.min.x, a.max.y)));
        }
        idx += l.chars().count() + 1;
    }
    None
}

/// One row as drawn: its tiles, its text, where it is in the body.
struct Row {
    tiles: Vec<Tile>,
    galley: std::sync::Arc<egui::Galley>,
    header: bool,
    top: f32,
    h: f32,
    margin: f32,
}

/// What the cursor highlight looks like over a row.
pub fn paint_hover(p: &egui::Painter, rect: Rect, dock: bool) {
    if dock {
        let ctx = p.ctx();
        let tex = widgets::linear_gradient(ctx, 90, &[(0.0, rgba(216, 180, 95, 0.28)), (1.0, rgba(216, 180, 95, 0.06))]);
        widgets::paint_texture(p, rect, 0.0, tex);
        widgets::outline(p, rect, 0.0, rgba(243, 217, 138, 0.55));
    } else {
        p.rect_filled(rect, 0.0, rgba(255, 212, 92, 0.2));
        widgets::outline(p, rect, 0.0, rgba(255, 212, 92, 0.6));
    }
}

/// The focus layer's highlight (`.popup .focused`).
pub fn paint_focused(p: &egui::Painter, rect: Rect) {
    p.rect_filled(rect, 3.0, rgba(255, 212, 92, 0.2));
    widgets::outline(p, rect, 3.0, rgba(255, 212, 92, 0.6));
}

impl Overlays {
    /// Draw one of crawl's menus; `top` is the one the keys go to.
    pub(crate) fn draw_menu(&mut self, ctx: &egui::Context, cx: &mut OverlayCx, menu: &MenuState, top: bool, depth: usize, screen: &Screen) {
        let font = screen.popup_font();
        let base = TextStyle::new(ctx, font, hex(0xdddddd));
        let pad_y = 0.5 * screen.rem();
        let pad_x = 0.8 * screen.rem();
        let rule = frame::rule_colour(cx.dock);
        let hov = if top { self.hovered(menu) } else { menu.last_hovered };
        let tp = menu.title_prompt.as_ref().filter(|t| !t.raw);
        // the title: crawl's words, or the filter's prompt and its field (menu.js title_prompt)
        let title_job = match tp {
            Some(t) => rich::plain(&t.prompt, &base, 0.0),
            None => rich::formatted(&menu.title.text, &base, 0.0),
        };
        // the rows' natural widths
        let row_pad = 0.4 * font;
        let gap = 0.4 * font;
        let li_min = 1.3 * font;
        let item_style = |it: &isorun_webtiles::MenuItem| -> (TextStyle, bool) {
            let header = it.level.unwrap_or(2) < 2;
            if header {
                (base.clone().colour(Color32::WHITE).bold(), true)
            } else {
                (base.clone().colour(isorun_ui::text::TERM_COLOURS[it.colour.unwrap_or(7) as usize & 15]), false)
            }
        };
        let mut natural: f32 = rich::natural_width(ctx, title_job.clone()) + if tp.is_some() { 8.0 * font } else { 0.0 };
        for it in menu.items.iter() {
            let w = match it {
                Some(it) => {
                    let (st, _) = item_style(it);
                    let tile_w = if it.tiles.as_ref().is_some_and(|t| !t.is_empty()) { 32.0 + gap } else { 0.0 };
                    tile_w + rich::natural_width(ctx, rich::formatted(&it.text, &st, 0.0))
                }
                None => rich::natural_width(ctx, rich::plain("...", &base, 0.0)),
            } + 2.0 * row_pad;
            natural = natural.max(w);
        }
        let more_style = base.clone().colour(hex(0xaaaaaa));
        let more_w = [&menu.more, &menu.alt_more].iter().map(|m| rich::natural_width(ctx, rich::formatted(&m.replacen("XXX", "top", 1), &more_style, 0.0))).fold(0.0, f32::max);
        natural = natural.max(more_w);
        // the box: shrink to fit between the popup's min and max width
        let scroll_key = menu.stack_order;
        let cw = widgets::ch(font);
        let mut min_content = frame::longest_word(&formatted_string_to_text(&menu.title.text)) as f32 * cw;
        for it in menu.items.iter().flatten() {
            let tile_w = if it.tiles.as_ref().is_some_and(|t| !t.is_empty()) { 32.0 + gap } else { 0.0 };
            min_content = min_content.max(tile_w + frame::longest_word(&formatted_string_to_text(&it.text)) as f32 * cw + 2.0 * row_pad);
        }
        min_content = min_content.max(frame::longest_word(&formatted_string_to_text(&menu.more)) as f32 * cw);
        let mut width = frame::shrink_to_fit(screen, natural + 2.0 * pad_x + 2.0, min_content + 2.0 * pad_x + 2.0, screen.popup_min_w(), screen.popup_max_w());
        let lay_rows = |width: f32| -> (Vec<Row>, f32) {
            let inner = width - 2.0 - 2.0 * pad_x;
            let mut rows = Vec::new();
            let mut y = pad_y;
            for (i, it) in menu.items.iter().enumerate() {
                let (tiles, galley, header, margin) = match it {
                    Some(it) => {
                        let (st, header) = item_style(it);
                        let tiles: Vec<Tile> = it.tiles.iter().flatten().map(Tile::from).collect();
                        let tile_w = if tiles.is_empty() { 0.0 } else { 32.0 + gap };
                        let g = rich::galley(ctx, rich::formatted(&it.text, &st, (inner - 2.0 * row_pad - tile_w).max(10.0)));
                        (tiles, g, header, if header { 0.3 * font } else { 0.0 })
                    }
                    None => (Vec::new(), rich::galley(ctx, rich::plain("...", &base, 0.0)), false, 0.0),
                };
                let _ = i;
                y += margin;
                let content = galley.size().y.max(if tiles.is_empty() { 0.0 } else { 32.0 }).max(li_min);
                let h = content + 0.1 * font;
                rows.push(Row { tiles, galley, header, top: y, h, margin });
                y += h;
            }
            // the web list ends a quarter em lower than its rows add up to (measured against the web build:
            // a one-row menu stands 4px taller there), so the box is as tall as it is there
            (rows, y + pad_y + 0.25 * font)
        };
        let (mut rows, mut content_h) = lay_rows(width);
        // the parts around the body
        let title_g = rich::galley(ctx, rich::formatted(&menu.title.text, &base, width - 2.0 - 2.0 * pad_x));
        let title_g = if tp.is_some() { rich::galley(ctx, title_job) } else { title_g };
        let title_h = pad_y * 2.0 + title_g.size().y.max(if tp.is_some() { widgets::normal_line_height(font) + 6.0 } else { 0.0 }) + 1.0;
        let max_h = screen.popup_max_h();
        let scroll_now = self.scroll.get(&scroll_key).copied().unwrap_or(0.0);
        // the more line takes its share first: shown from the overflow, which it changes
        let more_h_of = |t: &str, w: f32| -> f32 {
            if t.is_empty() {
                0.0
            } else {
                let g = rich::galley(ctx, rich::formatted(t, &more_style, w - 2.0 - 2.0 * pad_x));
                g.size().y + 0.8 * screen.rem() + 1.0
            }
        };
        let alt_h = more_h_of(&menu.alt_more.replacen("XXX", "top", 1), width);
        let mut overflow = title_h + content_h + alt_h + 2.0 > max_h;
        if overflow {
            // the scrollbar takes its width beside the rows
            width = (width + frame::SCROLLBAR_W).min(screen.popup_max_w());
            let r = lay_rows(width - frame::SCROLLBAR_W);
            rows = r.0;
            content_h = r.1;
        }
        let more_h = more_h_of(&more_text(menu, overflow, 0.0, 1.0), width);
        let mut body_h = content_h.min(max_h - title_h - more_h - 2.0).max(0.0);
        if !overflow && content_h > body_h + 1.0 {
            overflow = true;
        }
        body_h = body_h.floor();
        let height = title_h + body_h + more_h + 2.0;
        let rect = frame::place(screen, Vec2::new(width, height), depth, menu.ui_centred == Some(true), cx.touch && top);
        let max_scroll = (content_h - body_h).max(0.0);
        let mut scroll = scroll_now.min(max_scroll);
        if top {
            if let Some(s) = self.pending_menu_scroll.take() {
                scroll = s.clamp(0.0, max_scroll);
            }
        }
        let more = more_text(menu, overflow, scroll, max_scroll);
        let more_plain = formatted_string_to_text(&more);
        if top {
            self.menu_geom = MenuGeom { stack: menu.stack_order, rows: rows.iter().map(|r| (r.top, r.h)).collect(), client_h: body_h, scroll_h: content_h, inset: pad_y };
            let sw = switches(&more_plain);
            if self.menu_footer != sw {
                self.menu_footer = sw;
                if self.footer_index >= self.menu_footer.len() as i32 {
                    self.footer_index = -1;
                }
            }
        }
        let id = egui::Id::unique(("isorun-menu", menu.stack_order));
        let dock = cx.dock;
        let arrows = menu.flags & MenuFlag::ARROWS_SELECT != 0;
        let multi = menu.flags & MenuFlag::MULTISELECT != 0;
        let footer_index = if top { self.footer_index } else { -1 };
        let footer = self.menu_footer.clone();
        let mut clicked_row: Option<usize> = None;
        let mut clicked_switch: Option<String> = None;
        let mut new_scroll = scroll;
        let mut close = false;
        let field = if tp.is_some() { self.field_text_for_menu() } else { None };
        frame::layer(ctx, id, rect, |ui, _resp| {
            let p = ui.painter().clone();
            widgets::paint_panel(&p, rect, &frame::look(dock));
            let left = rect.left() + 1.0 + pad_x;
            // title
            let ty = rect.top() + 1.0 + pad_y;
            p.galley(Pos2::new(left, ty + (title_h - 1.0 - 2.0 * pad_y - title_g.size().y) / 2.0), title_g.clone(), base.colour);
            if tp.is_some() {
                // the filter's field beside its prompt (`.popup .title input.text`)
                let fx = left + title_g.size().x + 0.6 * font;
                let frect = Rect::from_min_max(Pos2::new(fx, ty), Pos2::new(rect.right() - 1.0 - pad_x, ty + widgets::normal_line_height(font) + 6.0));
                p.rect_filled(frect, 0.0, hex(0x111111));
                p.rect_stroke(frect, 0.0, egui::Stroke::new(1.0, hex(0x444444)), egui::StrokeKind::Inside);
                let g = rich::galley(ui.ctx(), rich::plain(field.as_deref().unwrap_or(""), &base.clone().colour(Color32::WHITE), 0.0));
                let tp0 = Pos2::new(frect.left() + 0.4 * font, frect.center().y - g.size().y / 2.0);
                p.galley(tp0, g.clone(), Color32::WHITE);
                p.rect_filled(Rect::from_min_size(Pos2::new(tp0.x + g.size().x + 1.0, tp0.y), Vec2::new(1.0, g.size().y)), 0.0, Color32::WHITE);
            }
            let by = rect.top() + 1.0 + title_h;
            widgets::rule(&p, rect.left() + 1.0..=rect.right() - 1.0, by - 1.0, rule);
            // the body, clipped to its box
            let body = Rect::from_min_size(Pos2::new(rect.left() + 1.0, by), Vec2::new(width - 2.0, body_h));
            new_scroll = frame::wheel(ui, body, content_h, scroll);
            let bp = p.with_clip_rect(body);
            let row_w = width - 2.0 - 2.0 * pad_x - if overflow { frame::SCROLLBAR_W } else { 0.0 };
            for (i, row) in rows.iter().enumerate() {
                let y = by + row.top - new_scroll;
                if y > body.bottom() || y + row.h < body.top() {
                    continue;
                }
                let r = Rect::from_min_size(Pos2::new(left, y), Vec2::new(row_w, row.h));
                let it = menu.items[i].as_ref();
                if it.and_then(|it| it.preselected).unwrap_or(false) {
                    bp.rect_filled(r, 0.0, rgba(60, 120, 255, 0.2));
                }
                if i as i64 == hov && footer_index < 0 {
                    paint_hover(&bp, r, dock);
                }
                let mut x = r.left() + row_pad;
                if !row.tiles.is_empty() {
                    tiles::paint(cx.tiles, &bp, &row.tiles, tiles::square(x, r.center().y, 32.0));
                    x += 32.0 + gap;
                }
                bp.galley(Pos2::new(x, r.center().y - row.galley.size().y / 2.0), row.galley.clone(), Color32::WHITE);
                let selectable = it.is_some_and(|it| it.level.unwrap_or(2) == 2 && (menu.tag == "use_item" || it.hotkeys.as_ref().is_some_and(|h| !h.is_empty()) || arrows));
                if selectable && body.contains(r.center()) && ui.interact(r.intersect(body), id.with(("row", i)), egui::Sense::click()).clicked() {
                    clicked_row = Some(i);
                }
                let _ = row.header;
                let _ = row.margin;
            }
            if overflow {
                new_scroll = frame::scrollbar(ui, id.with("bar"), body, content_h, new_scroll);
            }
            // the more line and its switches
            if more_h > 0.0 {
                let my = by + body_h;
                widgets::rule(&p, rect.left() + 1.0..=rect.right() - 1.0, my, rule);
                let g = rich::galley(ui.ctx(), rich::formatted(&more, &more_style, width - 2.0 - 2.0 * pad_x));
                let at = Pos2::new(left, my + 1.0 + 0.4 * screen.rem());
                for (k, sw) in footer.iter().enumerate() {
                    if let Some(cr) = char_rect(&g, &more_plain, sw.line, sw.col, sw.len) {
                        let cr = cr.translate(at.to_vec2());
                        if k as i32 == footer_index {
                            paint_hover(&p, cr, dock);
                        }
                        let resp = ui.interact(cr, id.with(("more", k)), egui::Sense::click());
                        if resp.hovered() && k as i32 != footer_index {
                            widgets::outline(&p, cr, 3.0, rgba(255, 212, 92, 0.35));
                        }
                        if resp.clicked() {
                            clicked_switch = Some(sw.key.clone());
                        }
                    }
                }
                p.galley(at, g, more_style.colour);
            }
            if cx.touch && top && !cx.watching {
                close = frame::touch_close(ui, id.with("close"), rect);
            }
        });
        if (new_scroll - scroll).abs() > 0.1 {
            self.scroll.insert(scroll_key, new_scroll);
            if top {
                self.schedule_scroll_sync();
            }
        } else {
            self.scroll.insert(scroll_key, scroll);
        }
        if !top {
            return;
        }
        if close {
            self.back(cx);
        }
        if let Some(k) = clicked_switch {
            self.send_more_switch(&k, cx);
        }
        if let Some(i) = clicked_row {
            let it = menu.items[i].as_ref().unwrap();
            if arrows {
                self.set_hover(menu, i as i64, true, true, cx);
                cx.send(cm::key(if multi { 32 } else { 13 }));
            } else if let Some(&hk) = it.hotkeys.as_ref().and_then(|h| h.first()) {
                cx.send(cm::key(hk as i32));
            }
        }
    }

    /// The filter's text, while the menu's title is a field (the field the keys type into).
    fn field_text_for_menu(&self) -> Option<String> {
        Some(self.menu_filter.clone())
    }

    /// A more-line switch: the key it prints, as the official client would send it.
    pub(crate) fn send_more_switch(&mut self, key: &str, cx: &mut OverlayCx) {
        let code = menu_nav::more_switch_keycode(key);
        if code != 0 {
            cx.send(cm::key(code));
        } else if key.chars().count() == 1 {
            cx.send(cm::input(key));
        }
    }

    // ------------------------------------------------------------------ navigation

    /// The menu with our hover in it, for the pure navigation.
    fn view(&self, menu: &MenuState) -> MenuState {
        MenuState { last_hovered: self.hovered(menu), ..menu.clone() }
    }

    /// menu.js update_visible_indices: the first and last row shown in full.
    pub(crate) fn menu_visible(&self) -> Visible {
        let g = &self.menu_geom;
        let n = g.rows.len() as i64;
        let mut vis = Visible { first: 0, last: (n - 1).max(0) };
        if n == 0 {
            return vis;
        }
        let scroll = self.scroll.get(&g.stack).copied().unwrap_or(0.0);
        let mut i = 0;
        while i < n {
            if g.rows[i as usize].0 - scroll + 1.0 >= 0.0 {
                vis.first = i;
                break;
            }
            i += 1;
        }
        while i < n {
            let (t, h) = g.rows[i as usize];
            if t + h - scroll - 1.0 >= g.client_h {
                break;
            }
            vis.last = i;
            i += 1;
        }
        vis
    }

    /// menu.js scroll_to_item / scroll_bottom_to_item: put row `index` at the top of the page
    /// (with the header right above it, if any), or its bottom at the bottom of the page.
    fn menu_scroll_to(&mut self, menu: &MenuState, index: i64, top: bool) {
        let g = self.menu_geom.clone();
        if g.rows.is_empty() {
            return;
        }
        let mut index = index.clamp(0, g.rows.len() as i64 - 1);
        if top && index > 0 && menu.items.get(index as usize - 1).and_then(|x| x.as_ref()).and_then(|x| x.level).unwrap_or(2) < 2 {
            index -= 1;
        }
        let (rt, rh) = g.rows[index as usize];
        let y = rt - g.rows[0].0;
        let inset = g.inset;
        let max = (g.scroll_h - g.client_h).max(0.0);
        let s = if top { y.max(0.0) } else { (y + rh + 2.0 * inset - g.client_h).max(0.0) }.min(max);
        self.scroll.insert(g.stack, s);
        self.schedule_scroll_sync();
    }

    /// menu.js snap_in_page: scroll only as far as needed to show the hover.
    fn snap_in_page(&mut self, menu: &MenuState, index: i64) {
        if index < 0 || index as usize >= self.menu_geom.rows.len() {
            return;
        }
        let vis = self.menu_visible();
        if index <= vis.first {
            self.menu_scroll_to(menu, index, true);
        } else if index >= vis.last {
            self.menu_scroll_to(menu, index, false);
        }
    }

    /// menu.js update_server_scroll, debounced 100 ms as there.
    pub(crate) fn schedule_scroll_sync(&mut self) {
        if self.scroll_sync_at.is_none() {
            self.scroll_sync_at = Some(self.now + 0.1);
        }
    }

    /// menu.js set_hover: move the hover and tell the server, only when it changes and the row can take it.
    pub(crate) fn set_hover(&mut self, menu: &MenuState, index: i64, mouse: bool, snap: bool, cx: &mut OverlayCx) {
        let n = menu.items.len() as i64;
        let index = if index >= n { (n - 1).max(0) } else { index };
        if menu.tag == GAME_MENU_TAG && index >= 0 {
            self.game_menu_last = index;
        }
        let can = index < 0 || menu_nav::item_selectable(menu, index) || pad_hoverable(menu, index);
        if index == self.hovered(menu) && can {
            self.footer_index = -1;
            return;
        }
        if can {
            self.hover.insert(menu.stack_order, (index, menu.last_hovered));
            self.footer_index = -1;
            if index >= 0 && snap {
                self.snap_in_page(menu, index);
            }
            cx.send(cm::menu_hover(index, mouse));
            self.schedule_scroll_sync();
        }
    }

    /// The top menu's cursor onto the row whose letter is `hotkey`, where there is one.
    pub fn hover_hotkey(&mut self, cx: &mut OverlayCx, hotkey: i64) {
        let Some(menu) = cx.state.menus.last() else { return };
        if let Some(at) = menu.items.iter().position(|it| it.as_ref().and_then(|i| i.hotkeys.as_ref()).and_then(|h| h.first()) == Some(&hotkey)) {
            if at as i64 != self.hovered(menu) {
                self.set_hover(menu, at as i64, false, true, cx);
            }
        }
    }

    /// A keyboard key while a server menu is up (menu.js's paging keys): true when it was the menu's.
    pub(crate) fn menu_key(&mut self, cx: &mut OverlayCx, ev: menu_nav::NavKey) -> bool {
        let Some(menu) = cx.state.menus.last() else { return false };
        if menu.hidden || menu.kind.as_deref() == Some("crt") {
            return false;
        }
        let Some(intent) = menu_nav::menu_key_intent(menu, ev) else { return false };
        self.menu_navigate(menu, intent, cx);
        true
    }

    /// The navigation itself, shared by the keyboard and the pad's bumpers.
    fn menu_navigate(&mut self, menu: &MenuState, key: MenuNavKey, cx: &mut OverlayCx) {
        let n = menu.items.len() as i64;
        let m = self.view(menu);
        match key {
            MenuNavKey::Up | MenuNavKey::Down => {
                let down = key == MenuNavKey::Down;
                if self.menu_footer_step(menu, if down { Step::Down } else { Step::Up }, cx) {
                } else if m.last_hovered < 0 {
                    // the menu opened without a hover: seat the cursor at the near end of the ring
                    let hov = hoverable(menu);
                    if hov.is_empty() {
                        // nothing can take a cursor (a list of text): the arrows scroll it
                        return self.menu_navigate(menu, if down { MenuNavKey::LineDown } else { MenuNavKey::LineUp }, cx);
                    }
                    if !down && !self.menu_footer.is_empty() {
                        self.enter_footer(false);
                    } else {
                        self.set_hover(menu, if down { hov[0] } else { *hov.last().unwrap() }, false, true, cx);
                    }
                } else {
                    let next = menu_nav::next_hoverable_item(&m, !down, m.last_hovered, false);
                    if next != -1 {
                        self.set_hover(menu, next, false, true, cx);
                    }
                }
            }
            MenuNavKey::LineUp | MenuNavKey::LineDown => {
                if n <= 0 {
                    return;
                }
                let vis = self.menu_visible();
                let t = if key == MenuNavKey::LineUp { menu_nav::line_up_target(vis) } else { menu_nav::line_down_target(menu, vis) };
                self.menu_scroll_to(menu, t, true);
                let snap = menu_nav::snap_hover_target(self.hovered(menu), self.menu_visible());
                if snap >= 0 {
                    self.set_hover(menu, snap, false, false, cx);
                }
            }
            MenuNavKey::PageUp | MenuNavKey::PageDown => {
                if n == 0 {
                    return;
                }
                let vis = self.menu_visible();
                let mut m = m;
                if m.last_hovered < 0 {
                    m.last_hovered = vis.first;
                    self.hover.insert(menu.stack_order, (vis.first, menu.last_hovered));
                }
                let mut rel = menu_nav::relative_hover(&m, vis);
                if key == MenuNavKey::PageUp {
                    // at the top already: no scroll, the hover goes to the top of the page
                    if vis.first == 0 {
                        rel = 0;
                    } else {
                        self.menu_scroll_to(menu, vis.first, false);
                    }
                } else if vis.last == n - 1 {
                    rel = -1;
                } else {
                    self.menu_scroll_to(menu, vis.last, true);
                }
                let t = menu_nav::relative_hover_target(&m, self.menu_visible(), rel);
                if t >= 0 {
                    self.set_hover(menu, t, false, true, cx);
                }
            }
            MenuNavKey::Home => {
                self.menu_scroll_to(menu, 0, true);
                let t = menu_nav::next_hoverable_item(&m, false, 0, true);
                if t >= 0 {
                    self.set_hover(menu, t, false, true, cx);
                }
            }
            MenuNavKey::End => {
                self.menu_scroll_to(menu, n - 1, false);
                let t = menu_nav::next_hoverable_item(&m, true, n - 1, true);
                if n > 0 && t >= 0 {
                    self.set_hover(menu, t, false, true, cx);
                }
            }
        }
        self.schedule_scroll_sync();
    }

    /// A step of the cursor on or onto the more line's switches, which are laid out as they are
    /// drawn: lines of columns. On them, up and down go to the nearest switch on the line above or
    /// below, left and right along the line, then on into the next column over. Off the top line up
    /// goes back to the last row; off the bottom line down comes round to the first. From the rows,
    /// down off the last lands on the top line's first switch and up off the first on the bottom
    /// line's. True when the cursor moved, or the step was the switches' to refuse.
    fn menu_footer_step(&mut self, menu: &MenuState, dir: Step, cx: &mut OverlayCx) -> bool {
        use crate::focus::{focus_step, FocusDir, FocusSlot, StepOpts};
        if self.menu_footer.is_empty() {
            return false;
        }
        let i = self.footer_index;
        let slots: Vec<FocusSlot> = self.menu_footer.iter().map(|f| FocusSlot { row: f.line as i32, col: f.col as i32 }).collect();
        let fd = match dir {
            Step::Up => FocusDir::Up,
            Step::Down => FocusDir::Down,
            Step::Left => FocusDir::Left,
            Step::Right => FocusDir::Right,
        };
        let sideways = matches!(dir, Step::Left | Step::Right);
        if i >= 0 {
            let next = focus_step(&slots, i, fd, StepOpts::default()).or_else(|| if sideways { focus_step(&slots, i, fd, StepOpts { columns: true, keep_column: false }) } else { None });
            if let Some(n) = next {
                self.footer_index = n as i32;
                return true;
            }
            if sideways {
                return true;
            }
            self.footer_index = -1;
            let m = self.view(menu);
            let row = menu_nav::next_hoverable_item(&m, dir == Step::Up, if dir == Step::Down { 0 } else { menu.items.len() as i64 - 1 }, true);
            if row >= 0 {
                self.hover.remove(&menu.stack_order);
                self.set_hover(menu, row, false, true, cx);
            }
            return true;
        }
        let m = self.view(menu);
        if sideways || m.last_hovered < 0 {
            return false;
        }
        // cycle_hover has nowhere left to go: the cursor steps off the rows onto the more line
        let next = menu_nav::next_hoverable_item(&m, dir == Step::Up, m.last_hovered, false);
        if next != -1 && next != m.last_hovered {
            return false;
        }
        self.enter_footer(dir == Step::Down);
        true
    }

    /// Onto the more line from the rows: down lands on its top line's first switch, up on its bottom line's.
    fn enter_footer(&mut self, down: bool) {
        let lines = self.menu_footer.iter().map(|f| f.line);
        let line = if down { lines.min() } else { lines.max() };
        let Some(line) = line else { return };
        self.footer_index = self.menu_footer.iter().enumerate().filter(|(_, f)| f.line == line).min_by_key(|(_, f)| f.col).map_or(-1, |(k, _)| k as i32);
    }

    /// The pad's and the keys' menu operations (overlays.ts `menuOp`).
    pub fn menu_op(&mut self, op: MenuOp, cx: &mut OverlayCx) {
        let Some(menu) = cx.state.menus.last() else { return };
        self.pointer_live = false;
        let arrows = menu.flags & MenuFlag::ARROWS_SELECT != 0;
        let multi = menu.flags & MenuFlag::MULTISELECT != 0;
        // the server's WRAP flag decides whether hover wraps at the ends (menu.js:353)
        let wrap = menu.flags & MenuFlag::WRAP != 0;
        let hov = hoverable(menu);
        let hovered = self.hovered(menu);
        let cur = hov.iter().position(|&h| h == hovered).map_or(-1, |p| p as i64);
        match op {
            MenuOp::Next | MenuOp::Prev => {
                let next = op == MenuOp::Next;
                if self.menu_footer_step(menu, if next { Step::Down } else { Step::Up }, cx) {
                    return;
                }
                if hov.is_empty() {
                    return cx.send(cm::key(if next { Keys::CK_DOWN } else { Keys::CK_UP }));
                }
                let len = hov.len() as i64;
                let n = if next {
                    if cur < 0 {
                        0
                    } else if cur + 1 < len {
                        cur + 1
                    } else if wrap {
                        0
                    } else {
                        cur
                    }
                } else if cur < 0 {
                    len - 1
                } else if cur > 0 {
                    cur - 1
                } else if wrap {
                    len - 1
                } else {
                    0
                };
                self.set_hover(menu, hov[n as usize], false, true, cx);
            }
            MenuOp::Left | MenuOp::Right => {
                // on the more line's switches they walk them; on the rows they are the keyboard's,
                // raw (the inventory's category pages, the `!` toggle elsewhere)
                if self.footer_index >= 0 {
                    self.menu_footer_step(menu, if op == MenuOp::Left { Step::Left } else { Step::Right }, cx);
                } else {
                    cx.send(cm::key(if op == MenuOp::Left { Keys::CK_LEFT } else { Keys::CK_RIGHT }));
                }
            }
            MenuOp::SectionNext | MenuOp::SectionPrev => {
                // menu.cc cycle_headers, both ways, on the client's hover; a menu without sections pages instead
                let forward = op == MenuOp::SectionNext;
                if !menu_nav::menu_has_sections(menu) {
                    return self.menu_navigate(menu, if forward { MenuNavKey::PageDown } else { MenuNavKey::PageUp }, cx);
                }
                let vis = self.menu_visible();
                let header = menu_nav::cycle_headers_target(&self.view(menu), forward, vis);
                if header < 0 {
                    return;
                }
                if !arrows || header < vis.first || header > vis.last {
                    self.menu_scroll_to(menu, header, true);
                }
                if arrows {
                    let t = menu_nav::next_hoverable_item(&self.view(menu), false, header, true);
                    if t >= 0 {
                        self.set_hover(menu, t, false, true, cx);
                    }
                }
                self.schedule_scroll_sync();
            }
            MenuOp::PageNext => self.menu_navigate(menu, MenuNavKey::PageDown, cx),
            MenuOp::PagePrev => self.menu_navigate(menu, MenuNavKey::PageUp, cx),
            MenuOp::First => {
                self.menu_navigate(menu, MenuNavKey::Home, cx);
                if let Some(&f) = hov.first() {
                    if self.hovered(menu) != f {
                        self.set_hover(menu, f, false, true, cx);
                    }
                }
            }
            MenuOp::Last => {
                self.menu_navigate(menu, MenuNavKey::End, cx);
                if let Some(&l) = hov.last() {
                    if self.hovered(menu) != l {
                        self.set_hover(menu, l, false, true, cx);
                    }
                }
            }
            MenuOp::Select => {
                if self.footer_index >= 0 {
                    let k = self.menu_footer[self.footer_index as usize].key.clone();
                    return self.send_more_switch(&k, cx);
                }
                let hk = menu.items.get(hovered.max(0) as usize).filter(|_| hovered >= 0).and_then(|x| x.as_ref()).and_then(|it| it.hotkeys.as_ref()).and_then(|h| h.first().copied());
                // what a click on the row does: an arrows menu acts on its hover (Space marks on a
                // multiselect, Enter takes it on the rest); with no row under the cursor, A leaves as Esc does
                if arrows && !multi && hovered < 0 {
                    cx.send(cm::key(27));
                } else if arrows {
                    cx.send(cm::key(if multi { 32 } else { 13 }));
                } else if let Some(hk) = hk {
                    cx.send(cm::key(hk as i32));
                } else {
                    cx.send(cm::key(13));
                }
            }
            MenuOp::AltSelect => {
                // the hovered row's letter, shifted: `A`..`Z` put a shop item on the shopping list
                if let Some(hk) = menu.items.get(hovered.max(0) as usize).filter(|_| hovered >= 0).and_then(|x| x.as_ref()).and_then(|it| it.hotkeys.as_ref()).and_then(|h| h.first().copied()) {
                    if (97..=122).contains(&hk) {
                        cx.send(cm::key(hk as i32 - 32));
                    }
                }
            }
            MenuOp::Examine => {
                // the hovered row described where it stands (cmd-keys.h: `'` is CMD_MENU_EXAMINE);
                // a menu without arrows-select drops a plain `menu_hover`, so the mouse form goes first there
                if self.footer_index >= 0 || hovered < 0 {
                    return;
                }
                if !menu.items.get(hovered as usize).and_then(|x| x.as_ref()).and_then(|it| it.hotkeys.as_ref()).is_some_and(|h| !h.is_empty()) {
                    return;
                }
                if !arrows {
                    cx.send(cm::menu_hover(hovered, true));
                }
                cx.send(cm::text_input("'"));
            }
            MenuOp::Toggle => cx.send(cm::key(32)),
            MenuOp::Cancel => cx.send(cm::key(27)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Up,
    Down,
    Left,
    Right,
}

/// The pad's menu operations (bindings.ts `{kind: 'menu', op}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuOp {
    Next,
    Prev,
    PageNext,
    PagePrev,
    SectionNext,
    SectionPrev,
    First,
    Last,
    Select,
    AltSelect,
    Examine,
    Toggle,
    Cancel,
    Left,
    Right,
}
