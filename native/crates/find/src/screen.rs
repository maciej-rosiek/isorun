//! The Find screen: crawl's search results (`Driver`) by category, ranked (`rank`), one tap to
//! go to one.
//!
//! - The panel takes a phone's whole safe area (less a margin) and stands in the middle of a
//!   bigger screen, as the dock's screens do. A tap beside it closes it (the game's tap-away).
//! - At the top: the title with crawl's count of matches, the close button; the search field
//!   (typing narrows the list at once; Enter or Search asks crawl itself, which also reads its
//!   annotations: `{potion}`, `rF+`, a shop's name); the sort (Smart: nearest level first, new
//!   things first in each; A-Z) and crawl's useless-and-duplicates switch (`=`).
//! - The categories with what crawl found in each, and All: a column beside the list where the
//!   panel is wide (a desktop, a phone on its side), chips over it where it is narrow (a phone
//!   held upright). Each shows the tile of the first result in it.
//! - The list: a heading for each level (the player's own, then by crawl's travel distance),
//!   rows with crawl's tile, its name in crawl's colour and what is worth knowing (new, carried
//!   already, more like it). A tap travels there (crawl's own pick); a long press (350 ms, the
//!   game's touch rule) or the mouse's hover shows the details card; a result under the player
//!   shows its card on a tap, as there is nowhere to go.
//! - Keys and the pad (the game routes them here, `op`): up and down walk the rows, left and
//!   right (LB, RB) the categories, A or Enter travels, X shows the card, Y switches the sort,
//!   B or Escape closes; a typed character goes into the search field; crawl's own `/` (sort)
//!   and `=` (useless) do what they do in its menu.

use crate::driver::Driver;
use crate::entry::{Category, Entry, Where};
use crate::rank::{counts, distance_words, kind_of, rank, Ranked, Sort, Tier};
use egui::{Color32, FontId, Pos2, Rect, Sense, Vec2};
use isorun_ui::theme::{GOLD, GOLD_BRIGHT, INK, INK_DIM};
use isorun_ui::tiles::TileIcons;
use isorun_ui::widgets::{self, palette, rgba};

/// What the screen asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum FindEvent {
    Travel(Entry),
    Close,
    /// ask crawl for these words (empty: everything)
    Search(String),
    ToggleUseless,
}

/// The pad's and the keys' operations (the game's router hands them over).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindOp {
    Next,
    Prev,
    /// the category before or after
    Left,
    Right,
    PageNext,
    PagePrev,
    First,
    Last,
    Select,
    Cancel,
    /// the details card of the lit row
    Details,
    /// Smart or A-Z
    Sort,
}

/// What the screen shows this frame.
pub struct View<'a> {
    pub driver: &'a Driver,
    /// the player's level, as crawl names levels (`entry::level_label`)
    pub here: &'a str,
    /// the UI scale setting
    pub scale: f32,
    /// the pad spoke last: its hints, and its cursor drawn
    pub pad: bool,
    /// a touch screen: taller rows, the touch hints
    pub touch: bool,
}

/// The screen's own state.
#[derive(Default)]
pub struct FindScreen {
    /// None: All
    pub category: Option<Category>,
    pub query: String,
    /// the words crawl searched for: its results are not narrowed by them again here (crawl
    /// matched what the screen cannot see, an item's annotations)
    searched: String,
    pub sort: Sort,
    cursor: usize,
    scroll: f32,
    cat_scroll: f32,
    /// the card of the lit row, shown by X or a tap on a result under the player
    details: bool,
    /// a key typed outside the field: the field takes the keys from the next frame
    focus_field: bool,
    /// the cursor moved by a key: its row is scrolled into view
    reveal: bool,
    panel: Option<Rect>,
    /// the rows of the last frame, in order
    rows: Vec<Ranked>,
    /// the categories of the last frame, All first
    cats: Vec<Option<Category>>,
    page_rows: usize,
    events: Vec<FindEvent>,
    /// the row whose card was drawn last frame (a test reads it)
    carded: Option<usize>,
    /// a key or the pad moved the cursor since the screen opened: it shows on touch too
    keyed: bool,
}

/// The search field's id (the game gives up its focus when the screen closes).
pub fn field_id() -> egui::Id {
    egui::Id::unique("isorun-find-field")
}

/// The chip's short words.
fn short(c: Category) -> &'static str {
    match c {
        Category::Jewellery => "Jewels",
        Category::Magic => "Magic",
        Category::OtherItems => "Other",
        Category::Portals => "Portals",
        c => c.label(),
    }
}

/// crawl's colour for a row (its 16 terminal colours), or the screen's ink.
fn colour_of(c: Option<i64>) -> Color32 {
    match c {
        None | Some(7) => INK,
        Some(c) => isorun_ui::text::TERM_COLOURS[c as usize & 15],
    }
}

/// The words beside a result: what makes it worth a look or not.
pub fn tags(r: &Ranked) -> Vec<String> {
    let e = &r.entry;
    let mut t = Vec::new();
    if r.tier == Tier::New {
        t.push(
            if e.category == Category::Shops {
                "not visited"
            } else if crate::rank::unidentified(e) {
                "unknown"
            } else if crate::rank::artefact(e) {
                "artefact"
            } else {
                "new"
            }
            .to_string(),
        );
    }
    if r.tier == Tier::Known {
        t.push("you carry one".into());
    }
    if e.duplicates > 0 {
        t.push(if e.piles > 1 { format!("+{} more in {} piles", e.duplicates, e.piles) } else { format!("+{} more", e.duplicates) });
    }
    if let Some(w) = e.waypoint {
        t.push(format!("waypoint {w}"));
    }
    t
}

/// Where, in words: the level and how far it is.
pub fn where_words(r: &Ranked, here: &str) -> String {
    if r.entry.place == Where::Here {
        return "right here".into();
    }
    match distance_words(here, &r.level) {
        Some(d) if d == "this level" => format!("{} · this level", r.level),
        Some(d) => format!("{} · {d}", r.level),
        None => r.level.clone(),
    }
}

/// The first tile of a category's results (the chip's icon).
fn first_tile(view: &View, c: Option<Category>) -> Option<isorun_webtiles::TileRef> {
    view.driver
        .entries
        .iter()
        .filter(|e| !matches!(e.place, Where::Carried(_)))
        .find(|e| c.is_none_or(|c| e.category == c))
        .and_then(|e| e.tiles.first().cloned())
}

impl FindScreen {
    pub fn new() -> Self {
        FindScreen { page_rows: 8, ..Default::default() }
    }

    /// The panel as last drawn (a tap outside it closes the screen).
    pub fn panel(&self) -> Option<Rect> {
        self.panel
    }

    /// The search field has the keys (the game leaves them to it).
    pub fn typing(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.has_focus(field_id()))
    }

    /// The screen opens again: a fresh search, the category and the sort as they were left.
    pub fn opened(&mut self) {
        self.query.clear();
        self.searched.clear();
        self.cursor = 0;
        self.scroll = 0.0;
        self.details = false;
        self.focus_field = false;
        self.keyed = false;
    }

    /// Close the screen whole (the pad's Select).
    pub fn close(&mut self) {
        self.events.push(FindEvent::Close);
    }

    /// The results as the screen lists them now.
    fn list(&self, view: &View) -> (Vec<Ranked>, Vec<(Option<Category>, usize)>) {
        let all = rank(&view.driver.entries, view.here, self.sort);
        let words: Vec<String> =
            if self.query.trim() != self.searched.trim() { self.query.to_lowercase().split_whitespace().map(str::to_string).collect() } else { Vec::new() };
        let found: Vec<Ranked> = all
            .into_iter()
            .filter(|r| {
                words.is_empty() || {
                    let h = r.entry.haystack();
                    words.iter().all(|w| h.contains(w.as_str()))
                }
            })
            .collect();
        let n = counts(&found);
        let mut cats: Vec<(Option<Category>, usize)> = vec![(None, found.len())];
        cats.extend(Category::ALL.iter().filter_map(|c| n.get(c).map(|k| (Some(*c), *k))));
        let rows = found.into_iter().filter(|r| self.category.is_none_or(|c| r.entry.category == c)).collect();
        (rows, cats)
    }

    /// A pad's or key's operation.
    pub fn op(&mut self, op: FindOp) {
        let n = self.rows.len();
        let step = |c: usize, d: isize| -> usize { (c as isize + d).clamp(0, n.saturating_sub(1) as isize) as usize };
        match op {
            FindOp::Next => self.cursor = step(self.cursor, 1),
            FindOp::Prev => self.cursor = step(self.cursor, -1),
            FindOp::PageNext => self.cursor = step(self.cursor, self.page_rows as isize),
            FindOp::PagePrev => self.cursor = step(self.cursor, -(self.page_rows as isize)),
            FindOp::First => self.cursor = 0,
            FindOp::Last => self.cursor = n.saturating_sub(1),
            FindOp::Left | FindOp::Right => {
                if !self.cats.is_empty() {
                    let at = self.cats.iter().position(|c| *c == self.category).unwrap_or(0) as isize;
                    let d = if op == FindOp::Left { -1 } else { 1 };
                    let k = self.cats.len() as isize;
                    self.set_category(self.cats[((at + d).rem_euclid(k)) as usize]);
                }
            }
            FindOp::Select => {
                if let Some(r) = self.rows.get(self.cursor).cloned() {
                    self.pick(&r);
                }
            }
            FindOp::Cancel => {
                if self.details {
                    self.details = false;
                } else {
                    self.events.push(FindEvent::Close);
                }
            }
            FindOp::Details => self.details = !self.details,
            FindOp::Sort => self.toggle_sort(),
        }
        self.reveal = true;
        self.keyed = true;
    }

    /// A character typed with the field not focused: crawl's menu keys for the sort and the
    /// useless switch, anything else goes into the search.
    pub fn hotkey(&mut self, key: &str) {
        match key {
            "/" => self.toggle_sort(),
            "Tab" => self.focus_field = true,
            "=" => self.events.push(FindEvent::ToggleUseless),
            k if k.chars().count() == 1 && !k.starts_with(char::is_control) => {
                self.query.push_str(k);
                self.focus_field = true;
            }
            _ => {}
        }
    }

    fn toggle_sort(&mut self) {
        self.sort = if self.sort == Sort::Smart { Sort::Name } else { Sort::Smart };
        self.cursor = 0;
        self.scroll = 0.0;
    }

    fn set_category(&mut self, c: Option<Category>) {
        self.category = c;
        self.cursor = 0;
        self.scroll = 0.0;
        self.details = false;
    }

    /// A row picked: travel there, or its card where it is under the player.
    fn pick(&mut self, r: &Ranked) {
        if r.entry.place == Where::Here {
            self.details = true;
        } else {
            self.events.push(FindEvent::Travel(r.entry.clone()));
        }
    }

    /// Ask crawl for the field's words.
    fn submit(&mut self) {
        self.searched = self.query.trim().to_string();
        self.category = None;
        self.cursor = 0;
        self.scroll = 0.0;
        self.events.push(FindEvent::Search(self.searched.clone()));
    }

    /// Draw the screen and answer the pointer; returns what it asks of the game.
    pub fn ui(&mut self, ctx: &egui::Context, view: &View, tiles: &mut TileIcons) -> Vec<FindEvent> {
        let (rows, cats) = self.list(view);
        if self.category.is_some() && !cats.iter().any(|(c, _)| *c == self.category) {
            // the category emptied (another search): back to All
            self.set_category(None);
        }
        self.rows = rows;
        self.cats = cats.iter().map(|(c, _)| *c).collect();
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
        let screen = isorun_ui::safe::rect(ctx);
        let s = view.scale.max(0.5);
        let compact = screen.width() < 700.0 || screen.height() < 560.0;
        let panel = if compact {
            screen.shrink(4.0)
        } else {
            let w = (980.0 * s).min(screen.width() * 0.94);
            let h = screen.height() * 0.9;
            Rect::from_min_size(Pos2::new((screen.center().x - w / 2.0).round(), (screen.top() + screen.height() * 0.05).round()), Vec2::new(w, h))
        };
        self.panel = Some(panel);
        let area = egui::Area::new(egui::Id::unique("isorun-find")).order(egui::Order::Foreground).fixed_pos(panel.min).constrain(false);
        let shown = area.show(ctx, |ui| {
            ui.set_clip_rect(screen);
            // the panel is solid: what is pressed on it is its own, never the world's
            ui.allocate_exact_size(panel.size(), Sense::click());
            self.draw(ui, panel, &cats, view, tiles, s);
        });
        ctx.move_to_top(shown.response.layer_id);
        std::mem::take(&mut self.events)
    }

    #[allow(clippy::too_many_arguments)]
    fn draw(&mut self, ui: &mut egui::Ui, panel: Rect, cats: &[(Option<Category>, usize)], view: &View, tiles: &mut TileIcons, s: f32) {
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        widgets::paint_panel(&p, panel, &widgets::PanelLook::dock_popup());
        let pad = 12.0 * s;
        let inner = panel.shrink2(Vec2::new(pad, 0.0));
        let mono = |size: f32| widgets::mono(&ctx, size * s);
        let caps = |size: f32| FontId::new(size * s, isorun_ui::fonts::caps());

        // the title, crawl's count, the close button
        let head_h = 46.0 * s;
        let title = p.layout_no_wrap("Find".into(), FontId::new(26.0 * s, isorun_ui::fonts::title()), GOLD_BRIGHT);
        p.galley(Pos2::new(inner.left(), panel.top() + (head_h - title.size().y) / 2.0 + 2.0), title.clone(), GOLD_BRIGHT);
        // what the list holds (crawl's own count takes in the pack, which the list leaves out)
        let found = cats.first().map_or(0, |c| c.1);
        let sub = if view.driver.busy() { "asking crawl…".to_string() } else { format!("{found} found · you are on {}", view.here) };
        let sub_g = p.layout_no_wrap(sub, mono(12.5), INK_DIM);
        let close = Rect::from_center_size(Pos2::new(inner.right() - 18.0 * s, panel.top() + head_h / 2.0), Vec2::splat(36.0 * s));
        let sub_x = inner.left() + title.size().x + 12.0 * s;
        if sub_x + sub_g.size().x < close.left() - 8.0 {
            p.galley(Pos2::new(sub_x, panel.top() + (head_h - sub_g.size().y) / 2.0 + 3.0), sub_g, INK_DIM);
        }
        if widgets::button(ui, close, egui::Id::unique("isorun-find-close"), "✕", widgets::ButtonLook::RoundClose).clicked() {
            self.events.push(FindEvent::Close);
        }
        let mut y = panel.top() + head_h;
        widgets::rule(&p, panel.left() + 1.0..=panel.right() - 1.0, y, palette::RULE);
        y += 8.0 * s;

        // the search field and its button; on a wide panel the switches beside them, where a
        // phone on its side has no height to spare for a row of their own
        let row_h = 38.0 * s;
        let go_w = 92.0 * s;
        let chip_h = 30.0 * s;
        let sort_label = if self.sort == Sort::Smart { "Sort: smart" } else { "Sort: A–Z" };
        let useless_label = if view.driver.hiding_useless { "Useless: hidden" } else { "Useless: shown" };
        let switch_w: Vec<f32> = [sort_label, useless_label].iter().map(|l| p.layout_no_wrap(l.to_string(), caps(12.5), INK).size().x + 24.0 * s).collect();
        let inline = panel.width() >= 760.0 * s;
        let switches_w = if inline { switch_w.iter().sum::<f32>() + 16.0 * s } else { 0.0 };
        let field = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width() - go_w - 8.0 * s - switches_w, row_h));
        p.rect_filled(field, 4.0, palette::IRON_DEEP);
        p.rect_stroke(field, 4.0, egui::Stroke::new(1.0, palette::BRONZE_DIM), egui::StrokeKind::Inside);
        let edit = egui::TextEdit::singleline(&mut self.query)
            .id(field_id())
            .font(mono(15.0))
            .text_color(INK)
            .hint_text(egui::RichText::new("Search: a name, a level, rF+…").color(INK_DIM))
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::symmetric((8.0 * s) as i8, 0))
            .align(egui::Align2::LEFT_CENTER)
            .desired_width(field.width());
        let resp = ui.put(field, edit);
        if self.focus_field {
            self.focus_field = false;
            resp.request_focus();
            if let Some(mut st) = egui::text_edit::TextEditState::load(&ctx, field_id()) {
                let end = egui::text::CCursor::new(self.query.chars().count());
                st.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                st.store(&ctx, field_id());
            }
        }
        let entered = resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let go = Rect::from_min_size(Pos2::new(field.right() + 8.0 * s, y), Vec2::new(go_w, row_h));
        if widgets::button(ui, go, egui::Id::unique("isorun-find-go"), "Search", widgets::ButtonLook::BronzePrimary).clicked() || entered {
            self.submit();
        }
        if !inline {
            y += row_h + 8.0 * s;
        }

        // the sort and crawl's switch for useless items
        let mut x = if inline { go.right() + 8.0 * s } else { inner.left() };
        let sy = if inline { y + (row_h - chip_h) / 2.0 } else { y };
        for (i, label) in [sort_label, useless_label].into_iter().enumerate() {
            let w = switch_w[i];
            let r = Rect::from_min_size(Pos2::new(x, sy), Vec2::new(w, chip_h));
            if widgets::button(ui, r, egui::Id::unique(("isorun-find-switch", i)), label, widgets::ButtonLook::Bronze).clicked() {
                if i == 0 {
                    self.toggle_sort();
                } else {
                    self.events.push(FindEvent::ToggleUseless);
                }
            }
            x += w + 8.0 * s;
        }
        y += if inline { row_h } else { chip_h } + 8.0 * s;

        // the categories: a column beside the list on a wide panel, chips over it on a narrow one
        let beside = panel.width() >= 620.0 * s;
        let foot_h = 26.0 * s;
        let body_bottom = panel.bottom() - foot_h;
        let first_tile = |c: Option<Category>| first_tile(view, c);
        let list_rect;
        if beside {
            let col_w = 220.0 * s;
            let col = Rect::from_min_max(Pos2::new(inner.left(), y), Pos2::new(inner.left() + col_w, body_bottom));
            let item_h = 36.0 * s;
            let content = cats.len() as f32 * item_h;
            self.cat_scroll = isorun_ui::scroll::scroll(ui, col, content, self.cat_scroll);
            let cp = p.with_clip_rect(col);
            for (i, (c, n)) in cats.iter().enumerate() {
                let r =
                    Rect::from_min_size(Pos2::new(col.left(), col.top() + i as f32 * item_h - self.cat_scroll), Vec2::new(col_w - 8.0 * s, item_h - 4.0 * s));
                if !r.intersects(col) {
                    continue;
                }
                let on = *c == self.category;
                let resp = ui.interact(r.intersect(col), egui::Id::unique(("isorun-find-cat", i)), Sense::click());
                self.paint_category(&cp, tiles, r, *c, *n, on, resp.hovered(), first_tile(*c), s, true);
                if isorun_ui::pointer::tap(&ctx, resp.clicked()) && !isorun_ui::scroll::dragging(ui, col) {
                    self.set_category(*c);
                }
            }
            self.cat_scroll = isorun_ui::scroll::bar(ui, egui::Id::unique("isorun-find-cat-bar"), col, content, self.cat_scroll);
            list_rect = Rect::from_min_max(Pos2::new(col.right() + 8.0 * s, y), Pos2::new(inner.right(), body_bottom));
        } else {
            // chips in rows, as many rows as they take, up to three shown (they scroll)
            let ch = 34.0 * s;
            let gap = 6.0 * s;
            let mut placed = Vec::new();
            let (mut cx, mut cy) = (0.0f32, 0.0f32);
            for (c, n) in cats {
                let label = format!("{} {n}", c.map_or("All", short));
                let w = p.layout_no_wrap(label, caps(12.5), INK).size().x + 30.0 * s + 16.0 * s;
                if cx > 0.0 && cx + w > inner.width() {
                    cx = 0.0;
                    cy += ch + gap;
                }
                placed.push((Rect::from_min_size(Pos2::new(cx, cy), Vec2::new(w, ch)), *c, *n));
                cx += w + gap;
            }
            let content = cy + ch;
            let shown_h = content.min(3.0 * ch + 2.0 * gap + if content > 3.0 * ch + 2.0 * gap { 0.5 * ch } else { 0.0 });
            let strip = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), shown_h));
            self.cat_scroll = isorun_ui::scroll::scroll(ui, strip, content, self.cat_scroll);
            let cp = p.with_clip_rect(strip);
            for (i, (r, c, n)) in placed.into_iter().enumerate() {
                let r = r.translate(Vec2::new(strip.left(), strip.top() - self.cat_scroll));
                if !r.intersects(strip) {
                    continue;
                }
                let on = c == self.category;
                let resp = ui.interact(r.intersect(strip), egui::Id::unique(("isorun-find-cat", i)), Sense::click());
                self.paint_category(&cp, tiles, r, c, n, on, resp.hovered(), first_tile(c), s, false);
                if isorun_ui::pointer::tap(&ctx, resp.clicked()) && !isorun_ui::scroll::dragging(ui, strip) {
                    self.set_category(c);
                }
            }
            self.cat_scroll = isorun_ui::scroll::bar(ui, egui::Id::unique("isorun-find-cat-bar"), strip, content, self.cat_scroll);
            y = strip.bottom() + 8.0 * s;
            widgets::rule(&p, panel.left() + 1.0..=panel.right() - 1.0, y - 4.0 * s, palette::RULE);
            list_rect = Rect::from_min_max(Pos2::new(inner.left(), y), Pos2::new(inner.right(), body_bottom));
        }

        self.draw_list(ui, list_rect, view, tiles, s);

        // the hints at the foot
        let hint = if view.pad {
            "A travel · LB RB category · X details · Y sort · B close"
        } else if view.touch {
            "Tap to travel there · hold for details"
        } else {
            "Enter travel · ← → category · Tab search · Esc close"
        };
        let g = p.layout_no_wrap(hint.into(), caps(11.5), INK_DIM);
        p.galley(Pos2::new(panel.center().x - g.size().x / 2.0, panel.bottom() - foot_h + (foot_h - g.size().y) / 2.0), g, INK_DIM);
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_category(
        &self,
        p: &egui::Painter,
        tiles: &mut TileIcons,
        r: Rect,
        c: Option<Category>,
        n: usize,
        on: bool,
        hot: bool,
        tile: Option<isorun_webtiles::TileRef>,
        s: f32,
        long: bool,
    ) {
        let fill = if on {
            rgba(74, 58, 30, 0.95)
        } else if hot {
            rgba(42, 36, 29, 0.95)
        } else {
            rgba(20, 17, 13, 0.9)
        };
        p.rect_filled(r, 6.0, fill);
        p.rect_stroke(r, 6.0, egui::Stroke::new(1.0, if on { palette::BRONZE } else { palette::BRONZE_DIM }), egui::StrokeKind::Inside);
        let icon = Rect::from_min_size(Pos2::new(r.left() + 4.0 * s, r.center().y - 13.0 * s), Vec2::splat(26.0 * s));
        let drawn = match (c, &tile) {
            (None, _) => tiles.id_of("CMD_SEARCH_STASHES").is_some_and(|id| tiles.paint_id(p, id, icon)),
            (_, Some(t)) => tiles.paint_ref(p, t, icon),
            _ => false,
        };
        if !drawn {
            if let Some(id) = c.map_or(&[][..], |c| c.icons()).iter().find_map(|n| tiles.id_of(n)) {
                tiles.paint_id(p, id, icon);
            }
        }
        let label = c.map_or("All", |c| if long { c.label() } else { short(c) });
        let colour = if on { GOLD_BRIGHT } else { INK };
        let font = FontId::new(12.5 * s, isorun_ui::fonts::caps());
        let g = p.layout_no_wrap(label.into(), font.clone(), colour);
        p.galley(Pos2::new(icon.right() + 6.0 * s, r.center().y - g.size().y / 2.0), g.clone(), colour);
        let cg = p.layout_no_wrap(n.to_string(), font, if on { GOLD_BRIGHT } else { GOLD });
        let cx = if long { r.right() - 8.0 * s - cg.size().x } else { icon.right() + 6.0 * s + g.size().x + 6.0 * s };
        p.galley(Pos2::new(cx, r.center().y - cg.size().y / 2.0), cg, if on { GOLD_BRIGHT } else { GOLD });
    }

    fn draw_list(&mut self, ui: &mut egui::Ui, view_rect: Rect, view: &View, tiles: &mut TileIcons, s: f32) {
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        let mono = |size: f32| widgets::mono(&ctx, size * s);
        if self.rows.is_empty() {
            let text = if view.driver.busy() {
                "Asking crawl what it knows…".to_string()
            } else if let Some(n) = &view.driver.notice {
                n.clone()
            } else if !self.query.trim().is_empty() {
                format!("Nothing here matches \u{201c}{}\u{201d}. Search asks crawl.", self.query.trim())
            } else {
                "Nothing found yet: explore, and what you see is listed here.".into()
            };
            let g = p.layout(text, mono(14.0), INK_DIM, view_rect.width() - 24.0 * s);
            p.galley(Pos2::new(view_rect.center().x - g.size().x / 2.0, view_rect.top() + 24.0 * s), g, INK_DIM);
            return;
        }
        let row_h = if view.touch { 52.0 } else { 44.0 } * s;
        let head_h = 28.0 * s;
        // the layout: a heading before each level in the smart sort
        let mut lay: Vec<(f32, Option<usize>)> = Vec::new();
        let mut y = 0.0;
        let mut last: Option<&str> = None;
        for (i, r) in self.rows.iter().enumerate() {
            if self.sort == Sort::Smart && last != Some(r.level.as_str()) {
                lay.push((y, None));
                y += head_h;
                last = Some(r.level.as_str());
            }
            lay.push((y, Some(i)));
            y += row_h;
        }
        let content = y;
        self.page_rows = ((view_rect.height() / row_h) as usize).max(1);
        if self.reveal {
            self.reveal = false;
            if let Some((top, _)) = lay.iter().find(|(_, i)| *i == Some(self.cursor)) {
                // its level's heading too, where it is the first of its level
                let head = if self.cursor == 0 || self.rows.get(self.cursor - 1).map(|r| &r.level) != self.rows.get(self.cursor).map(|r| &r.level) {
                    head_h
                } else {
                    0.0
                };
                if top - head < self.scroll {
                    self.scroll = top - head;
                } else if top + row_h > self.scroll + view_rect.height() {
                    self.scroll = top + row_h - view_rect.height();
                }
            }
        }
        self.scroll = isorun_ui::scroll::scroll(ui, view_rect, content, self.scroll);
        let lp = p.with_clip_rect(view_rect);
        let dragging = isorun_ui::scroll::dragging(ui, view_rect);
        let mut card: Option<(usize, Rect)> = None;
        let mut picked: Option<usize> = None;
        for (top, row) in &lay {
            let y = view_rect.top() + top - self.scroll;
            let h = if row.is_some() { row_h } else { head_h };
            if y + h < view_rect.top() || y > view_rect.bottom() {
                continue;
            }
            let Some(i) = *row else {
                // the level's heading
                let next = lay.iter().find(|(t, r)| *t > *top && r.is_some()).and_then(|(_, r)| *r).unwrap_or(0);
                let r = &self.rows[next];
                let text = if r.entry.place == Where::Here || r.level == view.here {
                    format!("{} — this level", r.level)
                } else {
                    where_words(r, view.here).replace(" · ", " — ")
                };
                let g = lp.layout_no_wrap(text.to_uppercase(), FontId::new(12.0 * s, isorun_ui::fonts::caps()), GOLD);
                lp.galley(Pos2::new(view_rect.left() + 4.0 * s, y + head_h - g.size().y - 5.0 * s), g, GOLD);
                lp.line_segment(
                    [Pos2::new(view_rect.left(), y + head_h - 2.0), Pos2::new(view_rect.right(), y + head_h - 2.0)],
                    egui::Stroke::new(1.0, palette::RULE),
                );
                continue;
            };
            let r = &self.rows[i];
            let rect = Rect::from_min_size(Pos2::new(view_rect.left(), y), Vec2::new(view_rect.width() - 8.0 * s, row_h - 2.0 * s));
            let resp = ui.interact(rect.intersect(view_rect), egui::Id::unique(("isorun-find-row", r.entry.index)), Sense::click());
            // the cursor: for the pad and the keys (on a desktop it rests on the first row, which
            // Enter takes), not under a finger until a key moved it
            let lit = i == self.cursor && (view.pad || !view.touch || self.keyed || self.details);
            if resp.hovered() && !isorun_ui::pointer::touch_mode(&ctx) {
                lp.rect_filled(rect, 4.0, rgba(255, 255, 255, 0.05));
            }
            if lit {
                lp.rect_filled(rect, 4.0, rgba(216, 180, 95, 0.12));
                lp.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, GOLD), egui::StrokeKind::Inside);
            }
            let icon = Rect::from_min_size(Pos2::new(rect.left() + 4.0 * s, rect.center().y - 16.0 * s), Vec2::splat(32.0 * s));
            for t in &r.entry.tiles {
                tiles.paint_ref(&lp, t, icon);
            }
            let tx = icon.right() + 10.0 * s;
            let right = rect.right() - 24.0 * s;
            let name = lp.layout(r.entry.name.clone(), mono(14.5), colour_of(r.entry.colour), right - tx);
            let mut sub = vec![where_words(r, view.here)];
            if self.sort == Sort::Smart {
                // the level's heading says where: the row says how far only for the player's own cell
                sub.clear();
                if r.entry.place == Where::Here {
                    sub.push("right here".into());
                }
            }
            sub.extend(tags(r));
            let sub_g = lp.layout_no_wrap(sub.join(" · "), mono(11.5), if r.tier == Tier::New { GOLD } else { INK_DIM });
            let block = name.size().y.min(row_h * 0.62) + if sub_g.size().x > 0.0 { sub_g.size().y + 1.0 } else { 0.0 };
            let ty = rect.center().y - block / 2.0;
            lp.with_clip_rect(Rect::from_min_max(Pos2::new(tx, rect.top()), Pos2::new(right, rect.bottom())).intersect(view_rect)).galley(
                Pos2::new(tx, ty),
                name.clone(),
                colour_of(r.entry.colour),
            );
            if sub_g.size().x > 0.0 {
                lp.with_clip_rect(Rect::from_min_max(Pos2::new(tx, rect.top()), Pos2::new(right, rect.bottom())).intersect(view_rect)).galley(
                    Pos2::new(tx, ty + name.size().y.min(row_h * 0.62) + 1.0),
                    sub_g,
                    INK_DIM,
                );
            }
            // the way there
            let arrow = if r.entry.place == Where::Here { "•" } else { "›" };
            let ag = lp.layout_no_wrap(arrow.into(), mono(20.0), palette::BRONZE);
            lp.galley(Pos2::new(rect.right() - 16.0 * s - ag.size().x / 2.0, rect.center().y - ag.size().y / 2.0), ag, palette::BRONZE);
            if isorun_ui::pointer::card_shows(&ctx, resp.hovered(), rect.intersect(view_rect)) {
                card = Some((i, rect));
            }
            if isorun_ui::pointer::tap(&ctx, resp.clicked()) && !dragging {
                self.cursor = i;
                picked = Some(i);
            }
        }
        self.scroll = isorun_ui::scroll::bar(ui, egui::Id::unique("isorun-find-bar"), view_rect, content, self.scroll);
        if let Some(i) = picked {
            let r = self.rows[i].clone();
            if r.entry.place == Where::Here && self.details {
                self.details = false;
            } else {
                self.pick(&r);
            }
        }
        // the details card: the long press's or the hover's, else the lit row's (X)
        let shown = card.or_else(|| {
            self.details
                .then(|| {
                    lay.iter().find(|(_, r)| *r == Some(self.cursor)).map(|(t, _)| {
                        (self.cursor, Rect::from_min_size(Pos2::new(view_rect.left(), view_rect.top() + t - self.scroll), Vec2::new(view_rect.width(), row_h)))
                    })
                })
                .flatten()
        });
        self.carded = shown.map(|(i, _)| i);
        if let Some((i, at)) = shown {
            let r = self.rows[i].clone();
            self.draw_card(&ctx, &r, at, view, tiles, s);
        }
    }

    fn draw_card(&self, ctx: &egui::Context, r: &Ranked, at: Rect, view: &View, tiles: &mut TileIcons, s: f32) {
        let screen = isorun_ui::safe::rect(ctx);
        let w = (360.0 * s).min(screen.width() - 16.0);
        let mono = |size: f32| widgets::mono(ctx, size * s);
        let pad = 12.0 * s;
        // the fonts first: `mono` reads the context, which `fonts_mut` holds locked
        let (big, small) = (mono(15.0), mono(12.5));
        let name = ctx.fonts_mut(|f| f.layout(r.entry.name.clone(), big, colour_of(r.entry.colour), w - 2.0 * pad - 48.0 * s));
        let mut lines = vec![(r.entry.category.label().to_string(), GOLD), (where_words(r, view.here), INK)];
        for t in tags(r) {
            lines.push((t, if r.tier == Tier::New { GOLD_BRIGHT } else { INK_DIM }));
        }
        if r.tier == Tier::Known {
            lines.push((format!("like your {}", kind_of(&r.entry.name)), INK_DIM));
        }
        lines.push((
            if r.entry.place == Where::Here {
                "It is under you.".into()
            } else if view.pad {
                "A: travel there".into()
            } else if view.touch {
                "Tap to travel there".into()
            } else {
                "Click or Enter: travel there".into()
            },
            INK_DIM,
        ));
        let gs: Vec<_> = lines.into_iter().map(|(t, c)| (ctx.fonts_mut(|f| f.layout(t, small.clone(), c, w - 2.0 * pad)), c)).collect();
        let h = pad * 2.0 + name.size().y.max(40.0 * s) + 6.0 * s + gs.iter().map(|(g, _)| g.size().y + 2.0).sum::<f32>();
        // under the row, or over it where there is no room below
        let y = if at.bottom() + 6.0 + h < screen.bottom() { at.bottom() + 6.0 } else { (at.top() - 6.0 - h).max(screen.top()) };
        let x = (at.left() + 24.0 * s).min(screen.right() - w - 8.0).max(screen.left() + 8.0);
        let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("isorun-find-card")));
        widgets::paint_panel(&p, rect, &widgets::PanelLook::dock_popup());
        let icon = Rect::from_min_size(rect.min + Vec2::splat(pad), Vec2::splat(40.0 * s));
        for t in &r.entry.tiles {
            tiles.paint_ref(&p, t, icon);
        }
        p.galley(Pos2::new(icon.right() + 8.0 * s, rect.top() + pad), name.clone(), colour_of(r.entry.colour));
        let mut y = rect.top() + pad + name.size().y.max(40.0 * s) + 6.0 * s;
        for (g, c) in gs {
            let hgt = g.size().y;
            p.galley(Pos2::new(rect.left() + pad, y), g, c);
            y += hgt + 2.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::Driver;
    use crate::entry::tests::{fixture, names};

    fn driver() -> Driver {
        let mut d = Driver::default();
        d.entries = crate::entry::parse_menu(&fixture(), &names);
        d.menu_up = true;
        d
    }

    fn frame(ctx: &egui::Context, screen: &mut FindScreen, d: &Driver, events: Vec<egui::Event>) -> Vec<FindEvent> {
        frame_at(ctx, screen, d, events, 0.0)
    }

    fn frame_at(ctx: &egui::Context, screen: &mut FindScreen, d: &Driver, events: Vec<egui::Event>, time: f64) -> Vec<FindEvent> {
        let mut tiles = TileIcons::default();
        let mut out = Vec::new();
        let raw =
            egui::RawInput { time: Some(time), events, screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(412.0, 915.0))), ..Default::default() };
        let mut o = ctx.run_pass(raw, |ctx| {
            let view = View { driver: d, here: "D:2", scale: 1.0, pad: false, touch: true };
            isorun_ui::pointer::note_frame(ctx, true);
            out = screen.ui(ctx, &view, &mut tiles);
        });
        o.textures_delta.clear();
        out
    }

    #[test]
    fn categories_and_the_pads_walk() {
        let ctx = egui::Context::default();
        let d = driver();
        let mut s = FindScreen::new();
        frame(&ctx, &mut s, &d, vec![]);
        // All, then each category with something in it, in their order
        assert_eq!(s.cats, vec![None, Some(Category::Weapons), Some(Category::Armour), Some(Category::Ammo), Some(Category::Misc), Some(Category::Stairs)]);
        assert_eq!(s.rows.len(), 19);
        s.op(FindOp::Right);
        frame(&ctx, &mut s, &d, vec![]);
        assert_eq!(s.category, Some(Category::Weapons));
        assert!(s.rows.iter().all(|r| r.entry.category == Category::Weapons));
        s.op(FindOp::Left);
        s.op(FindOp::Left);
        frame(&ctx, &mut s, &d, vec![]);
        assert_eq!(s.category, Some(Category::Stairs));
        // A travels to the lit row
        s.op(FindOp::Next);
        s.op(FindOp::Select);
        let ev = frame(&ctx, &mut s, &d, vec![]);
        assert!(matches!(&ev[..], [FindEvent::Travel(e)] if e.category == Category::Stairs));
        s.op(FindOp::Cancel);
        assert_eq!(frame(&ctx, &mut s, &d, vec![]), vec![FindEvent::Close]);
    }

    #[test]
    fn a_long_press_shows_the_card_and_does_not_travel() {
        let ctx = egui::Context::default();
        let d = driver();
        let mut s = FindScreen::new();
        frame(&ctx, &mut s, &d, vec![]);
        // the first row, under the level's heading
        let at = Pos2::new(200.0, 278.0);
        let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let touch = |phase| egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(1), phase, pos: at, force: None };
        frame_at(&ctx, &mut s, &d, vec![touch(egui::TouchPhase::Start), egui::Event::PointerMoved(at), press(true)], 1.0);
        assert_eq!(s.carded, None);
        frame_at(&ctx, &mut s, &d, vec![], 1.5);
        assert_eq!(s.carded, Some(0));
        let ev = frame_at(&ctx, &mut s, &d, vec![touch(egui::TouchPhase::End), press(false), egui::Event::PointerGone], 1.6);
        assert!(ev.is_empty(), "{ev:?}");
        // a quick tap travels
        frame_at(&ctx, &mut s, &d, vec![touch(egui::TouchPhase::Start), egui::Event::PointerMoved(at), press(true)], 3.0);
        let ev = frame_at(&ctx, &mut s, &d, vec![touch(egui::TouchPhase::End), press(false), egui::Event::PointerGone], 3.1);
        assert!(matches!(&ev[..], [FindEvent::Travel(_)]), "{ev:?}");
    }

    #[test]
    fn typed_words_narrow_the_list_then_ask_crawl() {
        let ctx = egui::Context::default();
        let d = driver();
        let mut s = FindScreen::new();
        s.hotkey("d");
        s.hotkey("a");
        s.hotkey("g");
        frame(&ctx, &mut s, &d, vec![]);
        assert!(s.rows.iter().all(|r| r.entry.name.contains("dag")));
        assert_eq!(s.rows.len(), 2);
        s.hotkey("=");
        assert_eq!(frame(&ctx, &mut s, &d, vec![]), vec![FindEvent::ToggleUseless]);
        s.submit();
        assert_eq!(frame(&ctx, &mut s, &d, vec![]), vec![FindEvent::Search("dag".into())]);
        // crawl's results for the words are not narrowed by them again
        assert_eq!(s.rows.len(), 19);
    }
}
