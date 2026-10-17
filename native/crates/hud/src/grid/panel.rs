//! Where the consumables action panel (action_panel.js) stands and how its cells are laid out.
//!
//! The official client floats the panel over the top-left corner of `#dungeon`, where Isorun's
//! stats pane already is. Isorun stands it in the strip along the top of the view between the
//! stats pane and the minimap: the one place at the top of the screen no pane holds, so the panel
//! reads as part of the same row of HUD furniture and the view under it stays clear. Nothing here
//! draws; the classic HUD (`classic.rs`) draws what this places, so the arithmetic can be pinned
//! by tests.

/// The panel wraps rather than truncating at one line (action_panel.js has the whole dungeon
/// width to run along; the strip between the panes is narrower, and a stocked character carries
/// more consumables than fit across it). It never takes more than this many lines, so the panel
/// stays a strip and the view keeps its top.
pub const PANEL_MAX_LINES: i32 = 4;
/// the least the strip may be for the panel to stand in it at all: one cell plus the gutters
const PANEL_MIN_CELLS: f32 = 2.0;

/// The strip the panel stands in: along the top of the free view, from the stats pane's right
/// edge to the minimap's left edge, a `gutter` clear of both. `stats_right` is None when the
/// stats pane has stepped aside and `minimap_left` is None when the sidebar has; then the strip
/// runs to the free view's own edge. A strip narrower than PANEL_MIN_CELLS cells is no strip:
/// the panel has nowhere to stand and gets zero width, which folds it away.
pub fn panel_span(free: egui::Rect, stats_right: Option<f32>, minimap_left: Option<f32>, gutter: f32, cell: f32) -> egui::Rect {
    let left = free.left().max(stats_right.map_or(free.left(), |r| r + gutter));
    let right = free.right().min(minimap_left.map_or(free.right(), |l| l - gutter));
    let width = right - left;
    if width < PANEL_MIN_CELLS * cell {
        return egui::Rect::from_min_size(egui::pos2(left, free.top()), egui::Vec2::ZERO);
    }
    egui::Rect::from_min_size(egui::pos2(left, free.top()), egui::vec2(width, free.height().max(0.0)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PanelGrid {
    /// cells across and down the panel
    pub cols: i32,
    pub rows: i32,
    /// how many of the items are drawn; the rest are behind the ellipsis
    pub shown: i32,
    /// action_panel.js: the run did not fit, so the ellipsis icon goes at the far end
    pub overflow: bool,
}

/// How `count` cells lie in a strip `width`×`height`, at `cell` a cell. Horizontal, the panel
/// fills a line across and wraps onto the next (PANEL_MAX_LINES of them at most, fewer when the
/// strip is short); vertical, it fills a column down and wraps onto the next column. What still
/// does not fit is behind the ellipsis, as action_panel.js draws it when its one line overflows.
pub fn panel_grid(count: i32, cell: f32, width: f32, height: f32, horizontal: bool) -> PanelGrid {
    let across = 1.max((width / cell).floor() as i32);
    let down = 1.max((height / cell).floor() as i32);
    let n = count.max(0);
    let (cols, rows) = if horizontal {
        let cols = across.min(n.max(1));
        (cols, down.min(PANEL_MAX_LINES).min(1.max((n + cols - 1) / cols)))
    } else {
        let rows = down.min(n.max(1));
        (across.min(PANEL_MAX_LINES).min(1.max((n + rows - 1) / rows)), rows)
    };
    let shown = n.min(cols * rows);
    PanelGrid { cols, rows, shown, overflow: n > shown }
}

/// The top-left corner of cell `i`, in panel points: horizontal fills a line at a time, vertical
/// a column at a time.
pub fn panel_cell_at(i: i32, grid: &PanelGrid, cell: f32, horizontal: bool) -> egui::Vec2 {
    let (col, row) = if horizontal { (i % grid.cols, i / grid.cols) } else { (i / grid.rows, i % grid.rows) };
    egui::vec2(col as f32 * cell, row as f32 * cell)
}

/// Which cell of the panel a point `x`, `y` from its top-left corner is in, or None when it is
/// past the last one.
pub fn panel_cell_index(x: f32, y: f32, grid: &PanelGrid, cell: f32, horizontal: bool) -> Option<i32> {
    if x < 0.0 || y < 0.0 || cell <= 0.0 {
        return None;
    }
    let col = (x / cell).floor() as i32;
    let row = (y / cell).floor() as i32;
    if col >= grid.cols || row >= grid.rows {
        return None;
    }
    let i = if horizontal { row * grid.cols + col } else { col * grid.rows + row };
    (i < grid.shown).then_some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_between_the_panes() {
        let free = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 800.0));
        let s = panel_span(free, Some(400.0), Some(800.0), 10.0, 32.0);
        assert_eq!((s.left(), s.width()), (410.0, 380.0));
        assert_eq!(panel_span(free, Some(400.0), Some(450.0), 10.0, 32.0).width(), 0.0);
    }

    #[test]
    fn wraps_and_overflows() {
        let g = panel_grid(30, 32.0, 320.0, 800.0, true);
        assert_eq!(g, PanelGrid { cols: 10, rows: 3, shown: 30, overflow: false });
        let g = panel_grid(50, 32.0, 320.0, 800.0, true);
        assert_eq!(g, PanelGrid { cols: 10, rows: 4, shown: 40, overflow: true });
        assert_eq!(panel_cell_at(12, &g, 32.0, true), egui::vec2(64.0, 32.0));
        assert_eq!(panel_cell_index(70.0, 40.0, &g, 32.0, true), Some(12));
        let v = panel_grid(5, 32.0, 320.0, 64.0, false);
        assert_eq!(v, PanelGrid { cols: 3, rows: 2, shown: 5, overflow: false });
        assert_eq!(panel_cell_index(70.0, 40.0, &v, 32.0, false), None);
    }
}
