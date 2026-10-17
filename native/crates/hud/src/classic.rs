//! The classic HUD's panes on the console grid (hud.ts): the WebTiles stats pane at the top-left
//! corner with the player's portrait, the right column (the minimap's slot, then the monster
//! list), the status strip beside the minimap and the trapped mark in the middle, the
//! consumables action panel between the stats pane and the minimap, the message pane along the
//! bottom with the scrim a `--more--` puts over the world, the pad's prompts in the view's
//! bottom-right corner and its menu buttons in the bottom corners, the status line, and the card
//! about the cell under the pointer. With the dock on, the stats pane and the action panel stand
//! down; the rest stays.

use crate::glyphs::{glyph_size, paint_glyph, paint_hold_progress, Glyph, PadKind};
use crate::grid::console::{CellRect, GameLayout, Grid};
use crate::grid::messages::pane_rows;
use crate::grid::panel::{panel_cell_at, panel_cell_index, panel_grid, panel_span, PanelGrid};
use crate::grid::rows::Row;
use crate::grid::stats::{portrait_rows, stats_rows, BarMemory};
use crate::grid::{paint_row, PANE_SHADOW};
use crate::icons::{self, Placed};
use crate::paint::{hex, inset, outer, rgba, s, sa, Border, BoxCache, BoxStyle, Layer, RadialSize, C};
use crate::style::{self, Face, TextStyle, BRONZE_BRIGHT, TRIM, TRIM_DARK};
use crate::{Device, HudCommand, Mode};
use egui::{Color32, Painter, Pos2, Rect, Sense, Vec2};
use isorun_scene::{rotate_dir, Attitude, Billboard, BillboardKind, Dir8, Scene, Threat, WoundLevel};
use isorun_ui::tiles::TileIcons;
use isorun_webtiles::{action_panel_items, cm, ActionPanelItem, GameState, Keys, MouseMode, UiState};

const ARROWS: [&str; 8] = ["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"];
/// monster_list.js: at most this many rows, then an ellipsis on the last
const MONSTER_LIST_MAX_ROWS: usize = 5;
/// monster_list.js: at most this many sprites in one grouped row
const MONSTER_LIST_MAX_SPRITES: usize = 6;
/// action_panel.js reserves two cells before the first item: the X that folds the panel away
/// (a right click opens the settings) and the game-menu button
const NUM_RESERVED_BUTTONS: i32 = 2;
/// the action panel's cell before `action_panel_scale`
const PANEL_CELL: f32 = 32.0;
/// the "Minimap tile size" and "Minimap size" settings' defaults
pub const MINIMAP_CELL_DEFAULT: u32 = 20;
pub const MINIMAP_TILES_DEFAULT: u32 = 19;
/// the status strip's badges are squares this many rows of the grid tall
pub const STATUS_BADGE_ROWS: f32 = 4.0;
/// the trapped mark is a square this fraction of the free view's shorter side
pub const TRAPPED_FRACTION: f32 = 0.8;
/// the most the minimap may take: half the screen across, this much of the column down
const MINIMAP_WIDEST: f32 = 0.5;
const MINIMAP_TALLEST: f32 = 0.7;
/// points between a panel's foot and the prompts under it
const BAR_GAP: f32 = 16.0;

/// the odd count within one of `n`, never under 1
fn odd_up(n: f32) -> i32 {
    2 * (n.max(0.0) / 2.0).floor() as i32 + 1
}
/// the biggest odd count not over `n`, never under 1
fn odd_down(n: f32) -> i32 {
    1.max(2 * (n.max(1.0) / 2.0).ceil() as i32 - 1)
}

/// The minimap's square on a sidebar column `side_w` × `side_h` on a `grid_w` wide screen, at
/// the "Minimap size" setting `tiles` and the "Minimap tile size" setting `cell` (hud.ts
/// `minimapBox`): that many `cell` tiles on a side, taken to an odd count so the player stands on
/// the centre tile, cut to the most whole tiles under both caps.
pub fn minimap_box(side_w: f32, side_h: f32, grid_w: f32, tiles: u32, cell: u32) -> f32 {
    if side_w <= 0.0 || side_h <= 0.0 {
        return 0.0;
    }
    let across = if tiles >= 1 { tiles } else { MINIMAP_TILES_DEFAULT } as f32;
    let px = if cell >= 1 { cell } else { MINIMAP_CELL_DEFAULT } as f32;
    let side = odd_up(across).min(odd_down((grid_w * MINIMAP_WIDEST).floor() / px)).min(odd_down((side_h * MINIMAP_TALLEST).floor() / px));
    side as f32 * px
}

/// One prompt of the pad's (bindings.ts `BindingLabel`): its button, its label (a crawl
/// formatted string), the hold's label, and whether it is a lesson rather than an action.
#[derive(Clone, Debug, PartialEq)]
pub struct PadPrompt {
    pub button: Glyph,
    pub label: String,
    pub hold: Option<String>,
    pub teaching: bool,
}

/// One line of the card about a cell (cell-info.ts `CellDetail`).
#[derive(Clone, Debug, PartialEq)]
pub struct InspectLine {
    pub text: String,
    /// `trivial`, `easy`, `tough`, `nasty`, `friendly`, `neutral` or `note`
    pub tone: Option<&'static str>,
    pub wound: Option<WoundLevel>,
}

/// The pad's hold in progress: which button, and how far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoldProgress {
    pub button: Glyph,
    pub fraction: f32,
}

/// What the panes are drawn from, besides the game state.
pub struct PaneInput<'a> {
    pub state: &'a GameState,
    pub scene: &'a Scene,
    pub facing: Dir8,
    pub mode: Mode,
    pub device: Device,
    pub pad: PadKind,
    pub spectating: bool,
    /// the dock is on: the stats pane and the action panel stand down
    pub dock: bool,
    /// the Hints setting is not off
    pub hints: bool,
    /// the pad's prompts for the corner (the game screen's `padLabels` or bindings `promptLabels`)
    pub prompts: &'a [PadPrompt],
    /// the pad's menu buttons for the bottom corners: Select on the left, Start, Y and X on the right
    pub menu_prompts: &'a [PadPrompt],
    pub holding: Option<HoldProgress>,
    /// a panel of the client's is up, its rect: the prompts stand under it
    pub panel: Option<Rect>,
    /// the card about the cell under the pointer, and what the clicks do there
    pub inspect: Option<(&'a [InspectLine], &'a [String])>,
    pub now: f64,
}

/// What the panes stood on last: the grid and its regions.
#[derive(Clone, Copy, Debug)]
pub struct PaneLayout {
    pub stats: Option<Rect>,
    pub sidebar: Option<Rect>,
    pub messages: Option<Rect>,
    /// the free view: the part no pane covers
    pub free: Rect,
    /// the free view widened to the view's right and bottom edges
    pub widened: Rect,
    /// the minimap's square, the slot the 2D renderer draws it in (empty with no sidebar)
    pub minimap: Rect,
    /// the action panel's strip
    pub panel: Rect,
    /// the corner the prompts stack up from
    pub prompts: Rect,
}

impl Default for PaneLayout {
    fn default() -> Self {
        PaneLayout { stats: None, sidebar: None, messages: None, free: Rect::NOTHING, widened: Rect::NOTHING, minimap: Rect::NOTHING, panel: Rect::NOTHING, prompts: Rect::NOTHING }
    }
}

pub struct Classic {
    layout: PaneLayout,
    grid: Option<Grid>,
    cells: GameLayout,
    bars: BarMemory,
    last_player_rev: u64,
    stats: Vec<Row>,
    portrait_cols: usize,
    log_offset: usize,
    status: Option<(String, f64)>,
    scrim: f32,
    panel_hover: Option<(i32, f64)>,
    pub minimap_hidden: bool,
}

impl Default for Classic {
    fn default() -> Self {
        Self::new()
    }
}

/// Tones of the monster list's names and the inspect card's lines.
fn attitude_colour(a: Option<Attitude>) -> C {
    match a {
        Some(Attitude::Neutral) => hex(0xffe08a),
        Some(Attitude::GoodNeutral) => hex(0xd8ffa0),
        Some(Attitude::Friendly) => hex(0x8aff8a),
        _ => hex(0xff8a8a),
    }
}

impl Classic {
    pub fn new() -> Classic {
        Classic { layout: PaneLayout::default(), grid: None, cells: GameLayout::default(), bars: BarMemory::default(), last_player_rev: u64::MAX, stats: vec![], portrait_cols: 0, log_offset: 0, status: None, scrim: 0.0, panel_hover: None, minimap_hidden: false }
    }

    /// Set the panes on the grid over the view (hud.ts `layout`): the stats pane on its cells at
    /// the top-left corner, the sidebar on its column at the right, the message pane on its rows
    /// along the bottom. `free` is the part of the view no pane lies over; the prompts stack up
    /// from the last row of `prompts`.
    #[allow(clippy::too_many_arguments)]
    pub fn layout(&mut self, grid: Grid, cells: GameLayout, free: CellRect, hide_stats: bool, hide_sidebar: bool, hide_messages: bool, prompts: CellRect, minimap_tiles: u32, minimap_cell: u32) -> PaneLayout {
        let free_px = grid.rect(free);
        let view_px = grid.rect(cells.view);
        let side_px = grid.rect(cells.sidebar);
        let stats_px = grid.rect(cells.stats);
        let w = minimap_box(side_px.width(), side_px.height(), grid.cols as f32 * grid.cw, minimap_tiles, minimap_cell);
        let sidebar = (!hide_sidebar && cells.sidebar.w > 0).then_some(side_px);
        let stats = (!hide_stats && cells.stats.w > 0).then_some(stats_px);
        let minimap = if sidebar.is_some() && w > 0.0 { Rect::from_min_size(egui::pos2(side_px.right() - w, side_px.top()), Vec2::splat(w)) } else { Rect::NOTHING };
        let panel = panel_span(free_px, stats.map(|r| r.right()), (minimap != Rect::NOTHING).then(|| minimap.left()), grid.cw, PANEL_CELL);
        let widened = Rect::from_min_size(free_px.min, egui::vec2(free_px.width().max(view_px.right() - free_px.left()), free_px.height().max(view_px.bottom() - free_px.top())));
        let portrait = if cells.stats.w > 0 { grid.ch * portrait_rows(cells.stats.w as usize) as f32 } else { 0.0 };
        let cols = if portrait > 0.0 { (cells.stats.w as usize).min((portrait / grid.cw).ceil() as usize + 1) } else { 0 };
        if self.grid != Some(grid) || self.cells != cells || cols != self.portrait_cols {
            self.last_player_rev = u64::MAX;
        }
        self.portrait_cols = cols;
        self.grid = Some(grid);
        self.cells = cells;
        self.layout = PaneLayout { stats, sidebar, messages: (!hide_messages).then(|| grid.rect(cells.messages)), free: free_px, widened, minimap, panel, prompts: grid.rect(prompts) };
        self.layout
    }

    pub fn pane_layout(&self) -> PaneLayout {
        self.layout
    }

    /// A short line in the middle of the screen's top for three seconds (hud.ts `status`).
    pub fn status(&mut self, text: impl Into<String>, now: f64) {
        self.status = Some((text.into(), now));
    }

    /// Read back through the log in the pane's own rows.
    pub fn scroll_log(&mut self, delta: i32) {
        self.log_offset = (self.log_offset as i32 - delta * 3).max(0) as usize;
    }

    /// Draw the panes and take their clicks.
    pub fn ui(&mut self, ui: &mut egui::Ui, inp: &PaneInput, tiles: &mut TileIcons, boxes: &mut BoxCache, out: &mut Vec<serde_json::Value>, commands: &mut Vec<HudCommand>) {
        let Some(grid) = self.grid else { return };
        let painter = ui.painter().clone();
        let st = inp.state;
        self.paint_trapped(&painter, inp, tiles);
        // the stats pane
        if let Some(rect) = self.layout.stats {
            if st.rev.player != self.last_player_rev {
                self.last_player_rev = st.rev.player;
                let (rows, prev) = stats_rows(&st.player, &st.options, self.bars, self.cells.stats.w as usize, self.portrait_cols);
                self.bars = prev;
                self.stats = rows;
            }
            self.paint_portrait(&painter, inp, tiles, rect, grid);
            let mut titled = vec![];
            for (i, row) in self.stats.iter().enumerate() {
                titled.extend(paint_row(&painter, row, egui::pos2(rect.left(), rect.top() + i as f32 * grid.ch), grid.cw, grid.ch, grid.font, style::term(7), true));
            }
            for (r, t) in titled {
                ui.interact(r, ui.scope_id().with(("stat-title", r.min.x as i32, r.min.y as i32)), Sense::hover()).on_hover_text(t);
            }
        }
        self.paint_status_strip(&painter, inp, tiles, grid);
        if !inp.dock {
            self.action_panel(ui, &painter, inp, tiles, out, commands);
        }
        if let Some(side) = self.layout.sidebar {
            self.monster_list(ui, &painter, inp, tiles, side, grid, commands);
        }
        // the scrim over the world while a --more-- waits, eased in and out over 400 ms
        let more = st.messages.more;
        let target = if more { 1.0 } else { 0.0 };
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        if (self.scrim - target).abs() > 1e-3 {
            self.scrim += (target - self.scrim).signum() * dt / 0.4;
            self.scrim = self.scrim.clamp(0.0, 1.0);
            ui.ctx().request_repaint();
        }
        if self.scrim > 0.0 && self.layout.messages.is_some() {
            let t = boxes.texture(ui.ctx(), "more-scrim", egui::vec2(160.0, 100.0), || BoxStyle { layers: vec![Layer::Radial(RadialSize::EllipseFarthest, (0.5, 0.5), vec![sa(rgba(3, 2, 6, 0.25)), sa(rgba(3, 2, 6, 0.55))])], ..Default::default() });
            painter.image(t.id(), ui.ctx().content_rect(), Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE.gamma_multiply(self.scrim));
        }
        if let Some(rect) = self.layout.messages {
            self.messages(ui, &painter, inp, rect, grid, out);
        }
        self.menus(&painter, inp, grid);
        self.prompt_bar(ui, &painter, inp, grid, commands);
        self.paint_status_line(&painter, ui.ctx(), inp.now);
        if let Some((lines, hints)) = inp.inspect {
            paint_inspect(ui.ctx(), boxes, lines, hints, style::hud_unit(ui.ctx().content_rect().size(), 1.0));
        }
    }

    /// The player's cell at the stats pane's top-left corner, as the 2D view draws it: the
    /// floor, the doll, the badges.
    fn paint_portrait(&self, painter: &Painter, inp: &PaneInput, tiles: &mut TileIcons, rect: Rect, grid: Grid) {
        let side = grid.ch * portrait_rows(self.cells.stats.w as usize) as f32;
        let sc = inp.scene;
        if !sc.player_on_level || side <= 0.0 {
            return;
        }
        let cell = Rect::from_min_size(rect.min, Vec2::splat(side));
        paint_cell(painter, tiles, sc, sc.player.0, sc.player.1, cell);
    }

    /// The status strip: the player's own corner badges blown up, leftward from the minimap.
    fn paint_status_strip(&self, painter: &Painter, inp: &PaneInput, tiles: &mut TileIcons, grid: Grid) {
        let mm = self.layout.minimap;
        if mm == Rect::NOTHING || st_glyphs(inp.state) {
            return;
        }
        let Some(me) = player_billboard(inp.scene) else { return };
        let badges: Vec<_> = me.status_icons.iter().filter(|i| !i.at_top && !i.square && !i.full).collect();
        if badges.is_empty() {
            return;
        }
        let px = grid.ch * STATUS_BADGE_ROWS;
        let right = mm.left() - grid.cw;
        let strip_left = self.layout.stats.map_or(self.layout.free.left(), |s| s.right() + grid.cw);
        let cols = (((right - strip_left) / px).floor() as usize).max(1).min(badges.len());
        let pad = (px * 0.1).round();
        for (i, b) in badges.iter().enumerate() {
            let x = right - ((i % cols) + 1) as f32 * px;
            let y = mm.top() + (i / cols) as f32 * px;
            paint_fit(painter, tiles, b.tile, Rect::from_min_size(egui::pos2(x, y), Vec2::splat(px)).shrink(pad), 1.0, true);
        }
    }

    /// The net or the web over the player, blown up in the middle of the free view.
    fn paint_trapped(&self, painter: &Painter, inp: &PaneInput, tiles: &mut TileIcons) {
        let st = inp.state;
        let covered = st.ui_state != UiState::NORMAL || !st.menus.is_empty() || !st.ui.is_empty();
        if covered || st_glyphs(st) {
            return;
        }
        let Some(me) = player_billboard(inp.scene) else { return };
        let free = self.layout.free;
        let px = (free.width().min(free.height()) * TRAPPED_FRACTION).round();
        let sq = Rect::from_center_size(free.center(), Vec2::splat(px));
        for m in me.status_icons.iter().filter(|i| i.full) {
            paint_fit(painter, tiles, m.tile, sq, 0.6, true);
        }
    }

    /// The consumables action panel (action_panel.js) in the strip between the stats pane and the
    /// minimap: the X and the game menu, then every item with an `action_panel_order`.
    fn action_panel(&mut self, ui: &mut egui::Ui, painter: &Painter, inp: &PaneInput, tiles: &mut TileIcons, out: &mut Vec<serde_json::Value>, commands: &mut Vec<HudCommand>) {
        let st = inp.state;
        let o = &st.options;
        let opt_bool = |k: &str| o.get(k).and_then(|v| v.as_bool());
        let inited = !st.player.inv.is_empty();
        let map_view = st.ui_state == UiState::VIEW_MAP;
        let box_ = self.layout.panel;
        if opt_bool("action_panel_disabled") == Some(true) || inp.spectating || !inited || box_.width() <= 0.0 || map_view {
            return;
        }
        let commanding = st.input_mode == MouseMode::COMMAND;
        if opt_bool("action_panel_show") == Some(false) {
            // action_panel.js hide_panel: only the placeholder button that brings it back
            let r = Rect::from_min_size(box_.min, egui::vec2(20.0, 20.0));
            painter.rect_filled(r, 3.0, rgba(0, 0, 0, 0.6).to_color32());
            painter.text(r.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::monospace(14.0), Color32::WHITE);
            if ui.interact(r, ui.scope_id().with("panel-show"), Sense::click()).on_hover_text("Show the action panel").clicked() {
                commands.push(HudCommand::PanelShown(true));
            }
            return;
        }
        let scale = o.get("action_panel_scale").and_then(|v| v.as_f64()).filter(|v| *v > 0.0).unwrap_or(100.0) as f32 / 100.0;
        let horizontal = o.get("action_panel_orientation").and_then(|v| v.as_str()) != Some("vertical");
        let font_size = o.get("action_panel_font_size").and_then(|v| v.as_f64()).filter(|v| *v > 0.0).unwrap_or(16.0) as f32;
        let items = action_panel_items(st);
        let cell = (PANEL_CELL * scale).round();
        let grid = panel_grid(items.len() as i32 + NUM_RESERVED_BUTTONS, cell, box_.width(), box_.height(), horizontal);
        let area = Rect::from_min_size(box_.min, egui::vec2(grid.cols as f32 * cell, grid.rows as f32 * cell));
        let resp = ui.interact(area, ui.scope_id().with("action-panel"), Sense::click());
        let hover = resp.hover_pos().and_then(|p| panel_cell_index(p.x - area.left(), p.y - area.top(), &grid, cell, horizontal));
        match (hover, self.panel_hover) {
            (Some(i), Some((j, _))) if i == j => {}
            (Some(i), _) => self.panel_hover = Some((i, inp.now)),
            (None, _) => self.panel_hover = None,
        }
        let at = |i: i32| area.min + panel_cell_at(i, &grid, cell, horizontal);
        let named = |tiles: &mut TileIcons, name: &str, i: i32| {
            if let Some(id) = icons::named(tiles, name) {
                let placed = tiles.rect(id).map(|rect| vec![Placed { rect, ox: 0.0, oy: 0.0 }]).unwrap_or_default();
                icons::paint(tiles, painter, &placed, Rect::from_min_size(at(i), Vec2::splat(cell)), Color32::WHITE);
            }
        };
        if grid.shown > 0 {
            named(tiles, "PROMPT_NO", 0);
        }
        if grid.shown > 1 {
            named(tiles, "CMD_GAME_MENU", 1);
        }
        for i in 0..2 {
            if hover == Some(i) {
                named(tiles, "CURSOR3", i);
            }
        }
        for (idx, item) in items.iter().take((grid.shown - NUM_RESERVED_BUTTONS).max(0) as usize).enumerate() {
            let i = idx as i32 + NUM_RESERVED_BUTTONS;
            let p = at(i);
            let pack = crate::gear::PackItem { item: item.item.clone(), letter: item.letter.clone() };
            let placed = icons::layer_tiles(tiles, &crate::quick_slots::item_tiles(&pack));
            icons::paint(tiles, painter, &placed, Rect::from_min_size(p, Vec2::splat(cell)), Color32::WHITE);
            // cell_renderer.js draw_quantity: white with a black shadow, top-left
            if let Some(q) = pack.count() {
                let g = painter.ctx().fonts_mut(|f| f.layout_no_wrap(q.clamp(0, 999).to_string(), egui::FontId::monospace(font_size * scale), Color32::WHITE));
                crate::paint::shadowed_galley(painter, p + egui::vec2(2.0, 2.0), g, Color32::WHITE, &[(1.0, 1.0, 2.0, Color32::BLACK)]);
            }
            if hover == Some(i) {
                named(tiles, "CURSOR3", i);
            }
            if pack.useless() {
                named(tiles, "OOR_MESH", i);
            }
            // Isorun: the inventory letter, bottom-right, so the palette and the panel name the same slot
            let lg = painter.ctx().fonts_mut(|f| f.layout_no_wrap(item.letter.clone(), egui::FontId::monospace((9.0 * scale).round().max(8.0)), Color32::WHITE));
            crate::paint::shadowed_galley(painter, p + egui::vec2(cell - 2.0 - lg.size().x, cell - 1.0 - lg.size().y), lg, Color32::from_white_alpha(217), &[(0.0, 0.0, 2.0, Color32::BLACK)]);
        }
        if grid.overflow {
            if let Some(id) = icons::named(tiles, "ELLIPSIS") {
                if let Some(rect) = tiles.rect(id) {
                    let size = egui::vec2(rect.w as f32, rect.h as f32) * scale;
                    let r = Rect::from_min_size(area.max - size, size);
                    let cellr = Rect::from_min_size(r.min - egui::vec2(rect.ox as f32, rect.oy as f32) * scale, Vec2::splat(rect.cell as f32 * scale));
                    tiles.paint(painter, &rect, cellr, Color32::WHITE);
                }
            }
        }
        // a tooltip on the cell after half a second
        if let Some((i, since)) = self.panel_hover {
            if inp.now - since > 0.5 {
                let text = panel_tip(i, &items);
                if !text.is_empty() {
                    let pos = resp.hover_pos().unwrap_or(area.center()) + egui::vec2(10.0, 10.0);
                    paint_small_tooltip(ui.ctx(), &text, pos);
                }
            } else {
                ui.ctx().request_repaint();
            }
        }
        if let (Some(i), true) = (hover, resp.clicked()) {
            match i {
                0 => commands.push(HudCommand::PanelShown(false)),
                1 if commanding => out.push(cm::main_menu_action()),
                _ if commanding => {
                    if let Some(item) = items.get((i - NUM_RESERVED_BUTTONS) as usize) {
                        out.push(cm::inv_item_action(item.item.slot));
                    }
                }
                _ => {}
            }
        }
        if let (Some(i), true) = (hover, resp.secondary_clicked()) {
            if i == 0 {
                commands.push(HudCommand::PanelSettings);
            } else if i >= NUM_RESERVED_BUTTONS && commanding {
                if let Some(item) = items.get((i - NUM_RESERVED_BUTTONS) as usize) {
                    out.push(cm::inv_item_describe(item.item.slot));
                }
            }
        }
        let _: PanelGrid = grid;
    }

    /// The WebTiles monster list under the minimap (monster_list.js): the visible monsters
    /// sorted and grouped, at most five rows, each a strip of up to six sprites and the name in
    /// its attitude's colour; Isorun's bearing arrow in front, and a tap faces the monster.
    #[allow(clippy::too_many_arguments)]
    fn monster_list(&self, ui: &mut egui::Ui, painter: &Painter, inp: &PaneInput, tiles: &mut TileIcons, side: Rect, grid: Grid, commands: &mut Vec<HudCommand>) {
        let st = inp.state;
        let o = &st.options;
        let groups = isorun_scene_webtiles::monster_groups(inp.scene, st);
        let invis = &st.map.invisible_monster_desc;
        let px = o.get("tile_cell_pixels").and_then(|v| v.as_f64()).filter(|v| *v > 0.0).unwrap_or(32.0);
        let vs = o.get("tile_viewport_scale").and_then(|v| v.as_f64()).filter(|v| *v > 0.0).unwrap_or(100.0);
        let w = ((px * vs) / 100.0).round().clamp(16.0, 64.0) as f32;
        let mut rows = groups.len().min(MONSTER_LIST_MAX_ROWS);
        if !invis.is_empty() {
            rows += 1;
        }
        let font = egui::FontId::monospace(grid.font);
        let em = grid.font;
        let row_h = w.max(grid.ch);
        let mut y = side.top() + if self.layout.minimap == Rect::NOTHING { 0.0 } else { self.layout.minimap.height() };
        // the rows as laid out first, so the dock's list can take the widest one's width
        struct RowDraw<'a> {
            arrow: String,
            group: Vec<&'a Billboard>,
            name: egui::text::LayoutJob,
            invis: bool,
            ellipsis: bool,
        }
        let mut draws = vec![];
        for i in 0..rows {
            if !invis.is_empty() && i == 0 {
                let mut job = isorun_ui::text::formatted(&format!("<magenta>{invis}</magenta>"), grid.font);
                job.wrap.max_width = f32::INFINITY;
                draws.push(RowDraw { arrow: "?".into(), group: vec![], name: job, invis: true, ellipsis: false });
                continue;
            }
            let group = &groups[if invis.is_empty() { i } else { i - 1 }];
            let first = group[0];
            let mon = isorun_scene_webtiles::monster_of(st, first);
            let name = match (group.len(), mon) {
                (1, Some(m)) => m.name.clone(),
                (n, Some(m)) => format!("{n} {}", m.plural.clone().unwrap_or_else(|| m.name.clone())),
                _ => first.name.clone().unwrap_or_default(),
            };
            let mut colour = attitude_colour(first.attitude).to_color32();
            let threat = first.threat.unwrap_or(Threat::Easy);
            if threat == Threat::Trivial {
                colour = colour.gamma_multiply(0.75);
            }
            let bold = matches!(threat, Threat::Tough | Threat::Nasty);
            let mut job = egui::text::LayoutJob::default();
            job.append(&name, 0.0, egui::TextFormat { font_id: if bold { egui::FontId::new(grid.font, isorun_ui::fonts::bold()) } else { font.clone() }, color: colour, underline: if threat == Threat::Nasty { egui::Stroke::new(1.0, colour) } else { egui::Stroke::NONE }, italics: threat == Threat::Invisible, ..Default::default() });
            let b = inp.scene.bearing_to(first.x, first.y);
            let rel = b.map_or(0, |b| rotate_dir(b, -(inp.facing as i32)) as usize);
            let arrow = if rel == 0 { "▲".to_string() } else { ARROWS[rel].to_string() };
            let ellipsis = i == rows - 1 && rows < groups.len() + if invis.is_empty() { 0 } else { 1 };
            draws.push(RowDraw { arrow, group: group.iter().take(MONSTER_LIST_MAX_SPRITES).copied().collect(), name: job, invis: false, ellipsis });
        }
        // the dock's list may reach left of its narrow column (a phone's), to six tenths of the
        // screen; a name longer than that is cut short rather than run off the edge
        let max_w = if inp.dock { side.width().max(0.6 * (side.right() - grid.ox)) } else { side.width() };
        let laid: Vec<(f32, std::sync::Arc<egui::Galley>)> = draws
            .iter()
            .map(|d| {
                let sprites = if d.invis { 1 } else { d.group.len().max(1) };
                let mut fixed = 1.2 * em + 0.4 * em + sprites as f32 * w + 0.4 * em + 0.4 * em;
                if d.ellipsis {
                    fixed += 0.4 * em + 3.0 * grid.cw;
                }
                let mut job = d.name.clone();
                job.wrap = egui::text::TextWrapping::truncate_at_width((max_w - fixed).max(3.0 * grid.cw));
                let g = ui.ctx().fonts_mut(|f| f.layout_job(job));
                ((fixed + g.size().x).min(max_w), g)
            })
            .collect();
        // with the dock the list is as wide as its widest row, at the column's right edge
        let widest = laid.iter().map(|l| l.0).fold(0.0, f32::max);
        for (d, (wdt, g)) in draws.iter().zip(laid) {
            let row = if inp.dock { Rect::from_min_size(egui::pos2(side.right() - widest, y), egui::vec2(widest, row_h)) } else { Rect::from_min_size(egui::pos2(side.left(), y), egui::vec2(wdt, row_h)) };
            if row.bottom() > side.bottom() {
                break;
            }
            painter.rect_filled(row, 0.0, rgba(0, 0, 0, 0.55).to_color32());
            let mut x = row.left();
            painter.text(egui::pos2(x + 0.6 * em, row.center().y), egui::Align2::CENTER_CENTER, &d.arrow, font.clone(), crate::style::GOLD_BRIGHT.to_color32().lerp_to_gamma(hex(0xffd45c).to_color32(), 1.0));
            x += 1.2 * em + 0.4 * em;
            if d.invis {
                if let Some(id) = icons::named(tiles, "UNSEEN_INVISIBLE") {
                    let placed = tiles.rect(id).map(|rect| vec![Placed { rect, ox: 0.0, oy: 0.0 }]).unwrap_or_default();
                    icons::paint(tiles, painter, &placed, Rect::from_min_size(egui::pos2(x, row.center().y - w / 2.0), Vec2::splat(w)), Color32::WHITE);
                }
                x += w;
            } else {
                for b in &d.group {
                    paint_cell(painter, tiles, inp.scene, b.x, b.y, Rect::from_min_size(egui::pos2(x, row.center().y - w / 2.0), Vec2::splat(w)));
                    x += w;
                }
            }
            x += 0.4 * em;
            let gw = g.size().x;
            painter.galley(egui::pos2(x, row.center().y - g.size().y / 2.0), g, Color32::WHITE);
            if d.ellipsis {
                painter.text(egui::pos2(x + gw + 0.4 * em, row.center().y), egui::Align2::LEFT_CENTER, "...", font.clone(), hex(0xaaaaaa).to_color32());
            }
            if !d.invis {
                if let Some(first) = d.group.first() {
                    if ui.interact(row, ui.scope_id().with(("monster-row", first.x, first.y)), Sense::click()).clicked() {
                        commands.push(HudCommand::SelectMonster { x: first.x, y: first.y });
                    }
                }
            }
            y += row_h;
        }
    }

    /// The message pane (grid/messages.rs): the last lines that fit, the more row under them;
    /// a click on it answers a pending `--more--` for the player.
    fn messages(&mut self, ui: &mut egui::Ui, painter: &Painter, inp: &PaneInput, rect: Rect, grid: Grid, out: &mut Vec<serde_json::Value>) {
        let log = &inp.state.messages;
        let rows_n = (self.cells.messages.h - 1).max(0) as usize;
        let pane = pane_rows(log, rows_n, self.cells.messages.w as usize, self.log_offset);
        self.log_offset = pane.offset;
        let white = hex(0xffffff);
        for (i, row) in pane.lines.iter().enumerate() {
            paint_row(painter, row, egui::pos2(rect.left(), rect.top() + i as f32 * grid.ch), grid.cw, grid.ch, grid.font, white, true);
        }
        if !pane.more.is_empty() {
            // the game waits on the --more--: a gold bar that pulses
            let text: String = pane.more.iter().map(|s| s.text.as_str()).collect();
            let g = painter.ctx().fonts_mut(|f| f.layout_no_wrap(text, egui::FontId::monospace(grid.font), Color32::WHITE));
            let at = egui::pos2(rect.left(), rect.top() + rows_n as f32 * grid.ch);
            let bar = Rect::from_min_size(at, egui::vec2(g.size().x + 1.2 * grid.font, grid.ch));
            let t = ((inp.now % 2.2) / 1.1) as f32;
            let k = crate::dock::cubic_bezier((0.42, 0.0, 0.58, 1.0), if t <= 1.0 { t } else { 2.0 - t });
            let glow = egui::epaint::RectShape::filled(bar.expand(3.0 * k), 3, rgba(232, 184, 74, 0.7 - 0.15 * k).to_color32()).with_blur_width(12.0 * k + 0.5);
            painter.add(glow);
            painter.rect_filled(bar, 3.0, hex(0xe8b84a).to_color32());
            painter.galley_with_override_text_color(egui::pos2(bar.left() + 0.6 * grid.font, bar.center().y - g.size().y / 2.0), g, hex(0x120c02).to_color32());
            ui.ctx().request_repaint();
        }
        if log.more && !inp.spectating && ui.interact(rect, ui.scope_id().with("messages-more"), Sense::click()).clicked() {
            out.push(isorun_webtiles::key_message(Keys::SPACE));
        }
    }

    /// The pad's standing menu buttons in the view's two bottom corners (hud.ts `renderMenus`).
    fn menus(&self, painter: &Painter, inp: &PaneInput, grid: Grid) {
        if inp.device != Device::Pad || inp.spectating || inp.mode != Mode::Command || inp.menu_prompts.is_empty() || inp.dock {
            return;
        }
        let em = grid.font;
        let w = self.layout.widened;
        let foot = self.layout.free.bottom() - 0.15 * em;
        let pitch = 1.2 * em * 1.45;
        let left: Vec<&PadPrompt> = inp.menu_prompts.iter().filter(|p| p.button == Glyph::Select).collect();
        let right: Vec<&PadPrompt> = [Glyph::Start, Glyph::Y, Glyph::X].iter().filter_map(|b| inp.menu_prompts.iter().find(|p| p.button == *b)).collect();
        for (side, list) in [(0, left), (1, right)] {
            let n = list.len();
            for (i, p) in list.iter().enumerate() {
                let y = foot - (n - i) as f32 * pitch - pitch;
                let gem = 1.45 * 1.3 * em;
                let label = isorun_ui::text::formatted(&p.label, em);
                let g = painter.ctx().fonts_mut(|f| f.layout_job(label));
                if side == 0 {
                    let x = w.left() + 0.3 * em;
                    paint_glyph(painter, p.button, inp.pad, egui::pos2(x, y), gem, style::term(7).to_color32());
                    crate::paint::shadowed_galley(painter, egui::pos2(x + glyph_size(p.button, gem).x + 0.55 * em, y + (gem - g.size().y) / 2.0), g, style::term(7).to_color32(), &PANE_SHADOW);
                } else {
                    let gs = glyph_size(p.button, gem);
                    let x = w.right() - 0.3 * em - gs.x;
                    paint_glyph(painter, p.button, inp.pad, egui::pos2(x, y), gem, style::term(7).to_color32());
                    crate::paint::shadowed_galley(painter, egui::pos2(x - 0.55 * em - g.size().x, y + (gem - g.size().y) / 2.0), g, style::term(7).to_color32(), &PANE_SHADOW);
                }
            }
        }
    }

    /// The contextual prompts (hud.ts `renderBar`): a stack up from the corner, the interact
    /// prompt lowest, labels left of their glyphs; under a panel, one line flush with its right
    /// edge. Only while the pad spoke last, never for a spectator.
    fn prompt_bar(&self, ui: &mut egui::Ui, painter: &Painter, inp: &PaneInput, grid: Grid, commands: &mut Vec<HudCommand>) {
        if inp.device != Device::Pad || inp.spectating || inp.prompts.is_empty() {
            return;
        }
        let em = grid.font;
        let gem = 1.45 * 1.3 * em;
        let colour = style::term(7).to_color32();
        let chip_size = |p: &PadPrompt| -> (Vec2, std::sync::Arc<egui::Galley>) {
            let g = painter.ctx().fonts_mut(|f| f.layout_job(isorun_ui::text::formatted(&p.label, em)));
            let gs = glyph_size(p.button, gem);
            (egui::vec2(gs.x + 0.55 * em + g.size().x, gs.y.max(g.size().y) + 0.1 * em), g)
        };
        let order = ["LSTICK", "LSTICK_UP", "DPAD", "RSTICK", "LB", "RB", "LT", "RT", "L3", "R3", "SELECT", "START", "Y", "X", "B", "A"];
        let rank = |g: Glyph| order.iter().position(|n| Glyph::parse(n) == Some(g)).unwrap_or(99);
        if let Some(panel) = inp.panel {
            // under a panel: one line in UNDER_ORDER, A at the right end, flush with its right edge
            let mut ps: Vec<(usize, &PadPrompt)> = inp.prompts.iter().enumerate().collect();
            ps.sort_by_key(|(_, p)| rank(p.button));
            let sizes: Vec<_> = ps.iter().map(|(_, p)| chip_size(p)).collect();
            let total: f32 = sizes.iter().map(|s| s.0.x).sum::<f32>() + 1.4 * em * (sizes.len().saturating_sub(1)) as f32;
            let foot = self.layout.prompts.bottom();
            let top = (panel.bottom() + BAR_GAP).min(foot - sizes.iter().map(|s| s.0.y).fold(0.0, f32::max));
            let mut x = panel.right() - total;
            for ((i, p), (sz, g)) in ps.into_iter().zip(sizes) {
                let r = Rect::from_min_size(egui::pos2(x, top), sz);
                self.chip(ui, painter, p, i, r, g, gem, em, true, inp, commands, 0.7);
                x += sz.x + 1.4 * em;
                let _ = colour;
            }
            return;
        }
        // the stack: from the corner up, its right edge a cell in from the view's right edge
        let corner = self.layout.prompts;
        let mut y = corner.bottom() - 0.15 * em;
        for (i, p) in inp.prompts.iter().enumerate() {
            let (sz, g) = chip_size(p);
            let r = Rect::from_min_size(egui::pos2(corner.right() - grid.cw - sz.x, y - sz.y), sz);
            self.chip(ui, painter, p, i, r, g, gem, em, false, inp, commands, 1.0);
            y -= sz.y + 0.2 * em;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn chip(&self, ui: &mut egui::Ui, painter: &Painter, p: &PadPrompt, i: usize, r: Rect, g: std::sync::Arc<egui::Galley>, gem: f32, em: f32, glyph_left: bool, inp: &PaneInput, commands: &mut Vec<HudCommand>, quiet: f32) {
        let holding = inp.holding.filter(|h| h.button == p.button);
        let alpha = if holding.is_some() { 1.0 } else { quiet };
        let colour = style::term(7).to_color32().gamma_multiply(alpha);
        let gs = glyph_size(p.button, gem);
        let (gx, tx) = if glyph_left { (r.left(), r.left() + gs.x + 0.55 * em) } else { (r.right() - gs.x, r.left()) };
        paint_glyph(painter, p.button, inp.pad, egui::pos2(gx, r.center().y - gs.y / 2.0), gem, Color32::WHITE.gamma_multiply(alpha));
        if let Some(h) = holding {
            paint_hold_progress(painter, egui::pos2(gx, r.center().y - gs.y / 2.0), gem, BRONZE_BRIGHT.to_color32(), h.fraction);
        }
        crate::paint::shadowed_galley(painter, egui::pos2(tx, r.center().y - g.size().y / 2.0), g, colour, &PANE_SHADOW);
        if !p.teaching && ui.interact(r, ui.scope_id().with(("prompt", i)), Sense::click()).on_hover_text(crate::glyphs::glyph_name(p.button, inp.pad)).clicked() {
            commands.push(HudCommand::Prompt(i));
        }
    }

    fn paint_status_line(&mut self, painter: &Painter, ctx: &egui::Context, now: f64) {
        let Some((text, since)) = &self.status else { return };
        if now - since > 3.0 {
            self.status = None;
            return;
        }
        ctx.request_repaint();
        let screen = isorun_ui::safe::rect(ctx);
        let size = 15.0;
        let g = ctx.fonts_mut(|f| f.layout_no_wrap(text.clone(), egui::FontId::monospace(size), Color32::WHITE));
        let r = Rect::from_center_size(egui::pos2(screen.center().x, screen.top() + screen.height() * 0.12 + (g.size().y + 0.6 * size) / 2.0), g.size() + egui::vec2(1.6 * size, 0.6 * size));
        painter.rect_filled(r, 4.0, rgba(80, 20, 20, 0.85).to_color32());
        painter.galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
    }
}

fn st_glyphs(st: &GameState) -> bool {
    st.options.get("tile_display_mode").and_then(|v| v.as_str()) == Some("glyphs")
}

/// The player's own billboard on the level.
pub fn player_billboard(scene: &Scene) -> Option<&Billboard> {
    scene.player_on_level.then(|| scene.billboards.iter().find(|b| b.kind == BillboardKind::Player && (b.x, b.y) == scene.player)).flatten()
}

/// A cell as the 2D view draws it, in miniature: its floor and feature, what stands on it (its
/// doll's or its monster's layers), and its badges.
pub fn paint_cell(painter: &Painter, tiles: &mut TileIcons, scene: &Scene, x: i32, y: i32, cell: Rect) {
    if let Some(c) = scene.cell(x, y) {
        let mut base = vec![c.floor_tile];
        base.extend(c.underlays.iter().copied());
        if let Some(f) = c.feature_tile {
            base.push(f);
        }
        for id in base {
            if let Some(rect) = tiles.rect(id) {
                tiles.paint(painter, &rect, cell, Color32::WHITE);
            }
        }
    }
    for b in scene.billboards_at(x, y).filter(|b| matches!(b.kind, BillboardKind::Monster | BillboardKind::Player)) {
        let placed = icons::doll_tiles(tiles, b.tile, &b.layers);
        icons::paint(tiles, painter, &placed, cell, Color32::WHITE);
        for i in &b.status_icons {
            if let Some(mut rect) = tiles.rect(i.tile) {
                rect.ox += i.ox.round() as i32;
                rect.oy += i.oy.round() as i32;
                tiles.paint(painter, &rect, cell, Color32::WHITE);
            }
        }
    }
}

/// A tile blown up to fit a square, its own pixels filling it (Render2d `drawTileFit`).
fn paint_fit(painter: &Painter, tiles: &mut TileIcons, id: u32, square: Rect, alpha: f32, shadow: bool) {
    let Some(mut rect) = tiles.rect(id) else { return };
    let side = rect.w.max(rect.h);
    rect.ox = ((side - rect.w) / 2) as i32;
    rect.oy = ((side - rect.h) / 2) as i32;
    rect.cell = side;
    if shadow {
        tiles.paint(painter, &rect, square.translate(egui::vec2(0.0, 1.0)), Color32::BLACK.gamma_multiply(0.7 * alpha));
    }
    tiles.paint(painter, &rect, square, Color32::WHITE.gamma_multiply(alpha));
}

/// action_panel.js show_tooltip: the buttons say what their clicks do, an item its letter and
/// name and then its own two clicks.
fn panel_tip(i: i32, items: &[ActionPanelItem]) -> String {
    match i {
        0 => "Left click: minimize\nRight click: open settings".into(),
        1 => "Left click: show main menu".into(),
        _ => {
            let Some(item) = items.get((i - NUM_RESERVED_BUTTONS) as usize) else { return String::new() };
            let verb = item.item.extra.get("action_verb").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
            let mut t = format!("{} - {}", item.letter, item.item.name.clone().unwrap_or_default());
            if !verb.is_empty() {
                t += &format!("\nLeft click: {verb}");
            }
            t + "\nRight click: describe"
        }
    }
}

/// mouse_control.js show_tooltip: a small box near the pointer (`.tooltip`).
fn paint_small_tooltip(ctx: &egui::Context, text: &str, pos: Pos2) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("hud-small-tip")));
    let size = 15.0 * 0.85;
    let g = ctx.fonts_mut(|f| f.layout(text.to_string(), egui::FontId::monospace(size), Color32::WHITE, 320.0));
    let r = Rect::from_min_size(pos, g.size() + egui::vec2(size, 0.4 * size));
    painter.rect_filled(r, 0.0, rgba(0, 0, 0, 0.9).to_color32());
    painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, hex(0x666666).to_color32()), egui::StrokeKind::Inside);
    painter.galley(r.min + egui::vec2(0.5 * size, 0.2 * size), g, Color32::WHITE);
}

/// How full a monster's bar is for each wound class.
fn wound_fill(w: WoundLevel) -> f32 {
    match w {
        WoundLevel::Uninjured => 1.0,
        WoundLevel::LightlyDamaged => 0.9,
        WoundLevel::ModeratelyDamaged => 0.7,
        WoundLevel::HeavilyDamaged => 0.5,
        WoundLevel::SeverelyDamaged => 0.3,
        WoundLevel::AlmostDead => 0.1,
    }
}

/// The card about the cell the pointer or the aim is on (hud.ts `showInspect`): at the top of
/// the view, a monster's name over a bar of its wounds, then what the clicks do there.
pub fn paint_inspect(ctx: &egui::Context, boxes: &mut BoxCache, lines: &[InspectLine], hints: &[String], u: f32) {
    if lines.is_empty() {
        return;
    }
    let screen = isorun_ui::safe::rect(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("hud-inspect")));
    let base = 17.0 * u;
    let tone = |t: Option<&str>| -> C {
        match t {
            Some("trivial") => hex(0x9a9a9a),
            Some("easy") => hex(0xe8e8e8),
            Some("tough") => hex(0xffd75e),
            Some("nasty") => hex(0xff5a4a),
            Some("friendly") => hex(0x7fd67f),
            Some("neutral") => hex(0x9fc4ff),
            Some("note") => hex(0xb8b0a0),
            _ => hex(0xffffff),
        }
    };
    let mut parts: Vec<(std::sync::Arc<egui::Galley>, Option<WoundLevel>, bool)> = vec![];
    for (i, l) in lines.iter().enumerate() {
        let st = if i == 0 {
            TextStyle::new(Face::Caps700, 22.0 * u, tone(l.tone)).spacing(0.05).line(1.2)
        } else {
            TextStyle::new(Face::Mono, if l.tone == Some("note") { base * 0.9 } else { base }, tone(l.tone)).line(1.2)
        };
        parts.push((st.galley(ctx, &l.text, None), l.wound, i == 0));
    }
    let hint = (!hints.is_empty()).then(|| TextStyle::new(Face::Mono, base * 0.8, BRONZE_BRIGHT).line(1.2).galley(ctx, &hints.join("  ·  "), None));
    let inner_w = parts.iter().map(|p| p.0.size().x).chain(hint.iter().map(|h| h.size().x)).fold(0.0, f32::max).max(220.0 * u - 36.0 * u);
    let mut h = 6.0 * u;
    for p in &parts {
        h += p.0.size().y;
        if p.1.is_some() {
            h += 4.0 * u + 8.0 * u + 5.0 * u;
        }
    }
    if let Some(hg) = &hint {
        h += 4.0 * u + 1.0 + 4.0 * u + hg.size().y;
    }
    h += 8.0 * u + 2.0;
    let w = inner_w + 36.0 * u + 2.0;
    let rect = Rect::from_min_size(egui::pos2(screen.center().x - w / 2.0, screen.top() + 28.0 * u), egui::vec2(w, h));
    boxes.paint_box(&painter, "inspect", rect, &BoxStyle {
        layers: vec![Layer::Linear(180.0, vec![sa(rgba(27, 22, 18, 0.94)), sa(rgba(12, 10, 8, 0.94))])],
        radius: [4.0 * u; 4],
        border: Border::all(1.0, TRIM),
        insets: vec![inset(0.0, 0.0, 0.0, 1.0, hex(0x000000))],
        outers: vec![outer(0.0, 6.0, 24.0, 0.0, rgba(0, 0, 0, 0.7))],
        ..Default::default()
    });
    let mut y = rect.top() + 1.0 + 6.0 * u;
    for (g, wound, first) in parts {
        let gh = g.size().y;
        let at = egui::pos2(rect.center().x - g.size().x / 2.0, y);
        if first {
            crate::paint::shadowed_galley(&painter, at, g, Color32::WHITE, &[(0.0, 2.0, 4.0, Color32::BLACK)]);
        } else {
            painter.galley(at, g, Color32::WHITE);
        }
        y += gh;
        if let Some(wl) = wound {
            y += 4.0 * u;
            let bar = Rect::from_min_size(egui::pos2(rect.left() + 1.0 + 18.0 * u, y), egui::vec2(inner_w, 8.0 * u));
            painter.rect_filled(bar, 2.0 * u, Color32::BLACK);
            painter.rect_stroke(bar, 2.0 * u, egui::Stroke::new(1.0, TRIM_DARK.to_color32()), egui::StrokeKind::Inside);
            let c = isorun_scene::bars::wound_colours(wl);
            let fill = Rect::from_min_size(bar.min + egui::vec2(1.0, 1.0), egui::vec2((bar.width() - 2.0) * wound_fill(wl), bar.height() - 2.0));
            let col = |v: u32| C::from_color32(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
            crate::paint::gradient_rect(&painter, fill, true, &[(0.0, col(c.top)), (1.0, col(c.bottom))]);
            y += 8.0 * u + 5.0 * u;
        }
    }
    if let Some(hg) = hint {
        y += 4.0 * u;
        painter.rect_filled(Rect::from_min_size(egui::pos2(rect.left() + 1.0 + 18.0 * u, y), egui::vec2(inner_w, 1.0)), 0.0, hex(0x3a2d16).to_color32());
        y += 1.0 + 4.0 * u;
        painter.galley(egui::pos2(rect.center().x - hg.size().x / 2.0, y), hg, Color32::WHITE);
    }
    let _ = s;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimap_square() {
        // 19 tiles of 20 points at 1600x1000 with a 42-cell column
        assert_eq!(minimap_box(404.0, 988.0, 1600.0, 19, 20), 380.0);
        // a tall setting is cut to the column
        assert_eq!(minimap_box(404.0, 300.0, 1600.0, 41, 20), 11.0 * 20.0);
        assert_eq!(minimap_box(0.0, 300.0, 1600.0, 19, 20), 0.0);
        assert_eq!(odd_up(18.0), 19);
        assert_eq!(odd_down(18.0), 17);
    }

    #[test]
    fn panel_tips() {
        assert!(panel_tip(0, &[]).starts_with("Left click: minimize"));
        assert_eq!(panel_tip(5, &[]), "");
    }
}
