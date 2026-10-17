//! The one console grid.
//!
//! The screen is a grid of console cells sized from the resolution and the text size. In a game
//! the dungeon view fills the whole grid and the panes lie over it, each on its own cells, sized
//! as the official client sizes them (game.js `layout`): the stats pane at the top-left corner, a
//! column of `enums.stat_width` character cells on the right holding the minimap and the monster
//! list, a message pane of as many lines as the server's `layout` message asks for along the
//! bottom. Measuring a cell is the host's job (`fit`); everything else is arithmetic on cells.

/// the grid: how many cells, how big each is, and where the top-left cell sits, in points
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub cols: i32,
    pub rows: i32,
    pub cw: f32,
    pub ch: f32,
    pub ox: f32,
    pub oy: f32,
    /// the font size the cell was measured at
    pub font: f32,
}

/// a rectangle of cells: `x`, `y` are the top-left cell, `w`, `h` the size in cells
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CellRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// game.js `stat_width` / `enums.stat_width`: the sidebar is this many character cells wide
pub const STAT_WIDTH: i32 = 42;
/// The least the dungeon view keeps, in cells, on a screen too narrow for a full sidebar beside
/// it. The official client never meets one (it is a desktop client); this is Isorun's own floor,
/// so a phone in portrait still shows the dungeon. `show_diameter` in game.js is 17 cells: the
/// view is meant to show that many at once.
const MIN_VIEW_COLS: i32 = 34;
const MIN_VIEW_ROWS: i32 = 8;

/// the console's smallest window, so a layout never has to cope with nothing
pub const MIN_COLS: i32 = 40;
pub const MIN_ROWS: i32 = 16;

/// the cell's height as a multiple of the font size: a console cell is about twice as tall as it is wide
const LINE_HEIGHT: f32 = 1.2;
/// the smallest font the grid falls back to
const MIN_PX: f32 = 8.0;

/// Fit a grid of `cw`×`ch` cells into `width`×`height`: as many whole cells as fit, centred by
/// whole pixels so what stands on the grid lands on its cells exactly.
pub fn fit_grid(width: f32, height: f32, cw: f32, ch: f32, font: f32) -> Grid {
    let cols = MIN_COLS.max((width / cw).floor() as i32);
    let rows = MIN_ROWS.max((height / ch).floor() as i32);
    Grid { cols, rows, cw, ch, ox: ((width - cols as f32 * cw) / 2.0).floor(), oy: ((height - rows as f32 * ch) / 2.0).floor(), font }
}

/// The grid for a screen `width`×`height` at the text size `font_px` (grid/host.ts `fit`), with
/// `advance` the width of one character of the grid font per point of size (DejaVu Sans Mono:
/// 0.602). When the screen is too small for the console's least window at that size, the size
/// comes down until it fits, so a layout never has to cope with fewer than MIN_COLS × MIN_ROWS.
pub fn fit(width: f32, height: f32, font_px: f32, advance: f32) -> Grid {
    let mut px = font_px.round();
    while px > MIN_PX && (((width / (px * advance)).floor() as i32) < MIN_COLS || ((height / (px * LINE_HEIGHT).round()).floor() as i32) < MIN_ROWS) {
        px -= 1.0;
    }
    fit_grid(width, height, px * advance, (px * LINE_HEIGHT).round(), px)
}

impl Grid {
    /// a rectangle of cells in points, relative to the grid's host
    pub fn rect(&self, r: CellRect) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(self.ox + r.x as f32 * self.cw, self.oy + r.y as f32 * self.ch), egui::vec2(r.w as f32 * self.cw, r.h as f32 * self.ch))
    }
}

/// the game screen's regions, all in cells of the one grid; the panes lie over the view
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct GameLayout {
    /// the dungeon view: the whole grid, the panes over it
    pub view: CellRect,
    /// The part of the view no pane lies over: left of the sidebar column, above the messages.
    /// What rides the view's edges (the action panel, the action bar) rides this rectangle's,
    /// and the level map is confined to it as game.js confines the map to the view.
    pub clear: CellRect,
    /// The message pane along the bottom, under the clear rectangle: `msg_rows` rows of messages
    /// and one more row for the `--more--` line, as game.js `layout` sizes it.
    pub messages: CellRect,
    /// the right column: the minimap at its top, the monster list under it, the chat at its foot
    pub sidebar: CellRect,
    /// the stats pane: the view's top-left corner, as wide as the sidebar, as many rows as the pane prints
    pub stats: CellRect,
}

/// the stats pane prints this many rows before its status lights wrap (game.html #stats: title,
/// god, hp, mp, 5 rows of two columns, weapon, quiver, status)
const STATS_ROWS: i32 = 13;

/// Lay the panes over the grid, each sized as game.js `layout` sizes it: the view is the whole
/// grid; the stats pane takes `stat_width` columns at the top-left corner; the sidebar (minimap,
/// monster list) takes `stat_width` columns on the right; the message pane takes `msg_rows` rows
/// of messages and the `--more--` row along the bottom, left of the sidebar; `clear` is what none
/// of the sidebar and the messages covers. `dock_rows` keeps that many rows along the bottom for
/// the dock, the message pane and the sidebar standing on top of them. On a grid too narrow for
/// a column beside MIN_VIEW_COLS of clear view, the column (and the stats pane with it) gives way
/// down to nothing.
pub fn game_split(grid: &Grid, msg_rows: i32, stat_width: i32, dock_rows: i32) -> GameLayout {
    let side = 0.max(stat_width.min(grid.cols - MIN_VIEW_COLS));
    let left_w = grid.cols - side;
    let dock = 0.max(dock_rows.min(grid.rows - MIN_VIEW_ROWS));
    let msgs = 2.max((msg_rows + 1).min(grid.rows - dock - 4));
    let clear_h = grid.rows - dock - msgs;
    // A screen too narrow for the column beside the view but tall (a phone held upright): the
    // stats pane takes the top, as wide as it is meant to be or the screen, and a narrower
    // column (the minimap, the monster list) stands under it at the right, lower, over the view; the
    // messages keep the whole width. A column of a few cells beside the view read as nothing.
    if side < stat_width && grid.rows >= STATS_ROWS + MIN_VIEW_ROWS + msgs + dock + 8 {
        let column = (grid.cols / 2).min(stat_width / 2);
        let below = STATS_ROWS.min(grid.rows);
        // the column starts a third of the way down what is left, clear of the player in the middle
        let top = below + (clear_h - below) * 2 / 5;
        return GameLayout {
            view: CellRect { x: 0, y: 0, w: grid.cols, h: grid.rows },
            clear: CellRect { x: 0, y: 0, w: grid.cols - column, h: clear_h },
            messages: CellRect { x: 0, y: clear_h, w: grid.cols, h: msgs },
            sidebar: CellRect { x: grid.cols - column, y: top, w: column, h: clear_h - top },
            stats: CellRect { x: 0, y: 0, w: grid.cols.min(stat_width), h: below },
        };
    }
    GameLayout {
        view: CellRect { x: 0, y: 0, w: grid.cols, h: grid.rows },
        clear: CellRect { x: 0, y: 0, w: left_w, h: clear_h },
        messages: CellRect { x: 0, y: clear_h, w: left_w, h: msgs },
        sidebar: CellRect { x: left_w, y: 0, w: side, h: grid.rows - dock },
        stats: CellRect { x: 0, y: 0, w: side, h: STATS_ROWS.min(grid.rows) },
    }
}

/// The level map (`X`) as game.js `toggle_full_window_dungeon_view` lays it out: the map keeps to
/// the clear part of the view, and grows over the sidebar with `tile_level_map_hide_sidebar` and
/// over the messages with `tile_level_map_hide_messages`, the panes stepping off the grid.
pub fn level_map_split(grid: &Grid, layout: &GameLayout, hide_sidebar: bool, hide_messages: bool) -> CellRect {
    CellRect { x: 0, y: 0, w: if hide_sidebar { grid.cols } else { layout.clear.w }, h: if hide_messages { grid.rows } else { layout.clear.h } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_at_1600x1000() {
        let g = fit(1600.0, 1000.0, 16.0, 0.602);
        assert_eq!((g.cols, g.rows, g.ch), (166, 52, 19.0));
        let l = game_split(&g, 5, STAT_WIDTH, 0);
        assert_eq!(l.sidebar, CellRect { x: 124, y: 0, w: 42, h: 52 });
        assert_eq!(l.messages, CellRect { x: 0, y: 46, w: 124, h: 6 });
        assert_eq!(l.clear.h, 46);
        let d = game_split(&g, 5, STAT_WIDTH, 10);
        assert_eq!(d.messages.y + d.messages.h, 42);
    }

    #[test]
    fn narrow_screens_lose_the_column() {
        let g = fit(390.0, 844.0, 16.0, 0.602);
        assert!(g.cols >= MIN_COLS);
        let l = game_split(&g, 5, STAT_WIDTH, 0);
        // the stats across the top, the column under them, the messages the whole width
        assert_eq!(l.stats, CellRect { x: 0, y: 0, w: g.cols.min(42), h: 13 });
        assert_eq!((l.sidebar.w, l.sidebar.x + l.sidebar.w, l.sidebar.y + l.sidebar.h), ((g.cols / 2).min(21), g.cols, l.messages.y));
        assert!(l.sidebar.y > 13);
        assert_eq!(l.messages.w, g.cols);
        // a short narrow window keeps the old split
        let s = fit(390.0, 420.0, 16.0, 0.602);
        assert_eq!(game_split(&s, 5, STAT_WIDTH, 0).sidebar.w, (s.cols - 34).max(0).min(42));
    }
}
