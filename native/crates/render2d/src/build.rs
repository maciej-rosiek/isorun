//! A scene as a list of things to draw (render-2d `render`, in the official client's layer
//! order), in the map's own points: the cell (x, y) covers (x, y) * cs to (x + 1, y + 1) * cs.
//! The list is built once for a scene and drawn every frame (`emit`), so a frame that only
//! pans or turns the map does no work per cell beyond placing its corners.
//!
//! The order is the canvas's order with one liberty that changes no pixel: within a pass, what
//! stays inside its own cell is grouped by its place in the cell's stack and then by atlas, as
//! two cells' squares never overlap. That turns a level of cells alternating floor and wall
//! atlases into a handful of meshes, one draw each. What spills out of its cell (a tall
//! feature, a big monster) keeps the level's own order after the rest of its pass, as the
//! canvas draws a lower row over an upper one.

use crate::atlas::{MapTiles, Sprite};
use crate::palette::{self, MinimapColours, TERM_COLOURS};
use egui::Color32;
use isorun_scene::{flash_of, minibar_rects, BillboardKind, CellKind, Minibars, Scene, SceneCell, SceneCursor, TileId, MINIBAR_CELL};
use std::collections::HashMap;

/// What a quad is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tex {
    /// A solid colour (egui's white texel).
    White,
    Atlas(u8),
    /// A glyph of the glyph modes, text rather than a quad.
    Glyph,
}

/// How a quad goes with a turned map (render-2d `render`'s `lean` and `upright`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    /// Painted on the ground: turns with it (the floor, the walls, the veils).
    Ground,
    /// Stands in its cell: upright on screen however the ground turns (monsters, items, the
    /// player, a freestanding feature).
    Upright,
    /// Built into the ground (a door, a staircase) or laid on it (the cursor): turned back as
    /// far as the view's `upright_yaw` says, all the way when it says nothing.
    Lean,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub tex: Tex,
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// Texture coordinates of `min` and `max`; for a glyph, the index of its `Glyph`.
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub colour: Color32,
    pub turn: Turn,
    /// The centre of the cell it belongs to: what an upright quad turns about.
    pub anchor: [f32; 2],
    /// The cell's base tile, which fills the square: alone it takes the seam margin.
    pub base: bool,
}

/// A glyph of the glyph modes, centred on its cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub text: String,
    pub colour: Color32,
    pub size: f32,
}

/// What a 2D view draws: the display mode and the pieces of the rc that bear on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Atlas tiles (`tile_display_mode = tiles`).
    Tiles,
    /// The cell glyphs in the terminal palette.
    Glyphs,
    /// Tile terrain with glyph foregrounds.
    Hybrid,
    /// Solid colour blocks, the official minimap's look; also what tiles fall back to with no
    /// tile source.
    Blocks,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub mode: Mode,
    pub colours: MinimapColours,
    pub glyph_colours: [Color32; 16],
}

impl Default for Style {
    fn default() -> Self {
        Style { mode: Mode::Tiles, colours: MinimapColours::default(), glyph_colours: TERM_COLOURS }
    }
}

impl Style {
    /// The mode and the minimap palette the rc options ask for (`tile_display_mode`,
    /// `tile_*_col`).
    pub fn from_options(options: &serde_json::Map<String, serde_json::Value>) -> Self {
        let mode = match options.get("tile_display_mode").and_then(|v| v.as_str()) {
            Some("glyphs") => Mode::Glyphs,
            Some("hybrid") => Mode::Hybrid,
            _ => Mode::Tiles,
        };
        Style { mode, colours: MinimapColours::from_options(options), glyph_colours: TERM_COLOURS }
    }
}

/// The list a scene is drawn from.
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub items: Vec<Item>,
    pub glyphs: Vec<Glyph>,
}

/// Each pushed item's sort key: its pass, whether it spills out of its cell, then (inside its
/// cell) its place in the cell's stack and its atlas, or (spilling) the order it came in.
type Key = (u8, bool, u32, Tex);

pub struct Builder<'a> {
    tiles: &'a mut MapTiles,
    cs: f32,
    list: DrawList,
    keys: Vec<Key>,
    pass: u8,
    seq: u32,
    /// How many items each cell has had this pass.
    stacks: HashMap<(i32, i32), u32>,
}

impl<'a> Builder<'a> {
    pub fn new(tiles: &'a mut MapTiles, cs: f32) -> Self {
        Builder { tiles, cs, list: DrawList::default(), keys: Vec::new(), pass: 0, seq: 0, stacks: HashMap::new() }
    }

    fn next_pass(&mut self) {
        self.pass += 1;
        self.stacks.clear();
    }

    /// The list in drawing order.
    pub fn finish(self) -> DrawList {
        let Builder { list: DrawList { items, glyphs }, keys, .. } = self;
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by_key(|&i| keys[i]);
        DrawList { items: order.into_iter().map(|i| items[i]).collect(), glyphs }
    }

    fn push(&mut self, cell: (i32, i32), item: Item) {
        let cs = self.cs;
        let (x0, y0) = (cell.0 as f32 * cs, cell.1 as f32 * cs);
        let e = cs * 1e-3;
        let spill = item.min[0] < x0 - e || item.min[1] < y0 - e || item.max[0] > x0 + cs + e || item.max[1] > y0 + cs + e;
        let n = self.stacks.entry(cell).or_insert(0);
        let key = if spill { (self.pass, true, self.seq, Tex::White) } else { (self.pass, false, *n, item.tex) };
        *n += 1;
        self.seq += 1;
        self.list.items.push(item);
        self.keys.push(key);
    }

    fn anchor(&self, cell: (i32, i32)) -> [f32; 2] {
        [(cell.0 as f32 + 0.5) * self.cs, (cell.1 as f32 + 0.5) * self.cs]
    }

    /// A solid square over part of a cell, in the cell's own points.
    pub fn fill(&mut self, cell: (i32, i32), at: [f32; 4], colour: Color32, turn: Turn, base: bool) {
        if colour.a() == 0 {
            return;
        }
        let (x0, y0) = (cell.0 as f32 * self.cs, cell.1 as f32 * self.cs);
        let item = Item {
            tex: Tex::White,
            min: [x0 + at[0], y0 + at[1]],
            max: [x0 + at[0] + at[2], y0 + at[1] + at[3]],
            uv_min: [0.0; 2],
            uv_max: [0.0; 2],
            colour,
            turn,
            anchor: self.anchor(cell),
            base,

        };
        self.push(cell, item);
    }

    /// render-2d `drawTile`: tile `id` in the cell, at its offset in crawl's frame plus
    /// `(ox, oy)` texels, cut at texel row `ymax` (a doll's legs in water), or with its opaque
    /// texels starting on the frame's top row (`at_top`, a damage bar). False when there is no
    /// such tile.
    #[allow(clippy::too_many_arguments)]
    pub fn tile(&mut self, cell: (i32, i32), id: TileId, offset: (f32, f32), ymax: Option<f32>, at_top: bool, colour: Color32, turn: Turn, base: bool) -> bool {
        let Some(s) = self.tiles.sprite(id) else { return false };
        self.sprite(cell, &s, offset, ymax, at_top, colour, turn, base);
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn sprite(&mut self, cell: (i32, i32), s: &Sprite, (ox, mut oy): (f32, f32), ymax: Option<f32>, at_top: bool, colour: Color32, turn: Turn, base: bool) {
        if at_top {
            oy = -s.oy;
        }
        let mut h = s.h;
        if let Some(ymax) = ymax {
            if ymax < s.oy + s.h {
                h = (ymax - s.oy).max(0.0);
            }
        }
        if h <= 0.0 {
            return;
        }
        let scale = self.cs / s.cell;
        let (x0, y0) = (cell.0 as f32 * self.cs, cell.1 as f32 * self.cs);
        let min = [x0 + (s.ox + ox) * scale, y0 + (s.oy + oy) * scale];
        let item = Item {
            tex: Tex::Atlas(s.atlas),
            min,
            max: [min[0] + s.w * scale, min[1] + h * scale],
            uv_min: s.uv,
            uv_max: [s.uv[0] + s.w * s.texel[0], s.uv[1] + h * s.texel[1]],
            colour,
            turn,
            anchor: self.anchor(cell),
            base,
        };
        self.push(cell, item);
    }

    fn glyph(&mut self, cell: (i32, i32), cs: f32, text: &str, colour: Color32) {
        let index = self.list.glyphs.len() as f32;
        self.list.glyphs.push(Glyph { text: text.to_string(), colour, size: (cs * 0.8).floor().max(1.0) });
        let a = self.anchor(cell);
        let item = Item { tex: Tex::Glyph, min: a, max: a, uv_min: [index, 0.0], uv_max: [index, 0.0], colour, turn: Turn::Upright, anchor: a, base: false };
        self.push(cell, item);
    }
}

/// The cells of a scene in the level's own order (row by row), all of them or those in an
/// inclusive range `(x0, x1, y0, y1)`. A range with more cells than the level walks the level.
pub fn cells_in(scene: &Scene, range: Option<(i32, i32, i32, i32)>) -> Vec<&SceneCell> {
    match range {
        Some((x0, x1, y0, y1)) if ((x1 - x0 + 1) as i64) * ((y1 - y0 + 1) as i64) < scene.cells.len() as i64 => {
            let mut out = Vec::new();
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if let Some(c) = scene.cell(x, y) {
                        out.push(c);
                    }
                }
            }
            out
        }
        _ => {
            let mut keys: Vec<_> = scene.cells.keys().copied().collect();
            // a cell's key is its row, then its column: in order, the level's rows
            keys.sort_unstable();
            keys.iter().map(|k| &scene.cells[k]).collect()
        }
    }
}

fn in_range(range: Option<(i32, i32, i32, i32)>, x: i32, y: i32) -> bool {
    range.is_none_or(|(x0, x1, y0, y1)| x >= x0 && x <= x1 && y >= y0 && y <= y1)
}

/// Everything of `scene` that does not move between its revisions, at `cs` points a cell, in
/// the cells of `range` (all when `None`).
pub fn build(scene: &Scene, tiles: &mut MapTiles, cs: f32, style: &Style, range: Option<(i32, i32, i32, i32)>) -> DrawList {
    let blocks = style.mode == Mode::Blocks || !tiles.has_source();
    let glyphs = style.mode == Mode::Glyphs && !blocks;
    let hybrid = style.mode == Mode::Hybrid && !blocks;
    let cells = cells_in(scene, range);
    let white = Color32::WHITE;
    let mut b = Builder::new(tiles, cs);
    // the cells: the terrain and what is built into it
    for cell in &cells {
        let at = (cell.x, cell.y);
        if blocks {
            b.fill(at, [0.0, 0.0, cs, cs], style.colours.cell(cell), Turn::Ground, true);
        } else if glyphs {
            glyph_cell(&mut b, cell, cs, GlyphBack::Fill, &style.glyph_colours);
        } else {
            tile_cell(&mut b, cell);
        }
    }
    b.next_pass();
    let on_cell = |b: &isorun_scene::Billboard| in_range(range, b.x, b.y);
    if blocks {
        // the colour blocks standing in for sprites are upright like the sprites: square on
        // screen over a turned ground, not diamonds among the ground's cells
        for bb in scene.billboards.iter().filter(|b| on_cell(b)) {
            if let Some(c) = style.colours.billboard(bb) {
                b.fill((bb.x, bb.y), [0.0, 0.0, cs, cs], c, Turn::Upright, false);
            }
        }
    } else if !glyphs {
        // things standing in cells, in scene order, then their badges
        for bb in scene.billboards.iter().filter(|b| on_cell(b)) {
            let hidden_by_glyph = hybrid && !matches!(bb.kind, BillboardKind::Cloud | BillboardKind::Projectile);
            if hidden_by_glyph {
                continue;
            }
            let alpha = bb.alpha.unwrap_or(if bb.kind == BillboardKind::Cloud { 0.75 } else { 1.0 });
            let colour = if alpha != 1.0 { palette::white_alpha(alpha) } else { white };
            if bb.layers.is_empty() {
                b.tile((bb.x, bb.y), bb.tile, (0.0, 0.0), None, false, colour, Turn::Upright, false);
            } else {
                for l in &bb.layers {
                    b.tile((bb.x, bb.y), l.tile, (l.ox, l.oy), l.ymax, false, colour, Turn::Upright, false);
                }
            }
        }
        b.next_pass();
        if hybrid {
            // hybrid: the glyph stands in for the sprite, so it goes under the badges
            for cell in &cells {
                if scene.billboards_at(cell.x, cell.y).any(|b| !matches!(b.kind, BillboardKind::Cloud | BillboardKind::Projectile)) {
                    glyph_cell(&mut b, cell, cs, GlyphBack::Shade, &style.glyph_colours);
                }
            }
            b.next_pass();
        }
        // status badges over the sprite (or over the glyph standing in for it)
        for bb in scene.billboards.iter().filter(|b| on_cell(b)) {
            for i in &bb.status_icons {
                b.tile((bb.x, bb.y), i.tile, (i.ox, i.oy), None, i.at_top, white, Turn::Upright, false);
            }
        }
    }
    b.next_pass();
    if !blocks {
        // cell badges (travel trail, new stairs, exclusions) and veils, above everything at the cell
        for cell in &cells {
            let at = (cell.x, cell.y);
            if !glyphs {
                for &i in &cell.icons {
                    b.tile(at, i, (0.0, 0.0), None, false, white, Turn::Ground, false);
                }
            }
            if let Some(v) = palette::veil(cell) {
                b.fill(at, [0.0, 0.0, cs, cs], v, Turn::Ground, false);
            }
            if cell.kind == CellKind::Unknown {
                continue;
            }
            // the flash covers the whole cell, over everything drawn in it (cell_renderer.js `render_flash`)
            let f = flash_of(Some(cell));
            if f[3] > 0.0 {
                let c = |v: f32| (v * 255.0).round() as u8;
                b.fill(at, [0.0, 0.0, cs, cs], palette::rgba(c(f[0]), c(f[1]), c(f[2]), f[3]), Turn::Ground, false);
            }
        }
    }
    b.next_pass();
    // the player's own cell on a minimap: the player stands upright like any other thing in a
    // cell, so the mark stays square over a turned ground
    if blocks && scene.player_on_level && in_range(range, scene.player.0, scene.player.1) {
        b.fill(scene.player, [0.0, 0.0, cs, cs], style.colours.player, Turn::Upright, false);
    }
    b.finish()
}

/// What goes behind a glyph: the cell's black square in glyph mode, hybrid's dark backing
/// over the tile.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GlyphBack {
    Fill,
    Shade,
}

/// render-2d `drawGlyphCell`: the server's glyph and colour for the cell, upright.
fn glyph_cell(b: &mut Builder, cell: &SceneCell, cs: f32, back: GlyphBack, colours: &[Color32; 16]) {
    let at = (cell.x, cell.y);
    match back {
        GlyphBack::Fill => b.fill(at, [0.0, 0.0, cs, cs], Color32::BLACK, Turn::Upright, true),
        // hybrid: a dark square so the glyph reads over the tile
        GlyphBack::Shade => b.fill(at, [cs * 0.15, cs * 0.1, cs * 0.7, cs * 0.8], palette::rgba(0, 0, 0, 0.75), Turn::Upright, false),
    }
    let g = cell.glyph.as_deref().unwrap_or(" ");
    if g.is_empty() || g == " " {
        return;
    }
    let col = (cell.colour.unwrap_or(7) & 15) as usize;
    b.glyph(at, cs, g, colours[col]);
}

/// render-2d `drawTileCell`. The ground (floor, walls, their overlays) turns with the map, and
/// the feature built into a cell (a door, a fountain, an altar, stairs) is part of it: it leans
/// back only as far as the view's rule says. A feature that stands on the dungeon
/// (`freestanding`: a statue, a tree) stands upright in its cell like the monsters.
fn tile_cell(b: &mut Builder, cell: &SceneCell) {
    if cell.kind == CellKind::Unknown {
        return;
    }
    let at = (cell.x, cell.y);
    let w = Color32::WHITE;
    // the base tile fills the square, so it alone takes the seam margin
    if cell.kind == CellKind::Wall {
        b.tile(at, cell.wall_tile.unwrap_or(cell.floor_tile), (0.0, 0.0), None, false, w, Turn::Ground, true);
        for &o in &cell.wall_overlays {
            b.tile(at, o, (0.0, 0.0), None, false, w, Turn::Ground, false);
        }
        return;
    }
    b.tile(at, cell.floor_tile, (0.0, 0.0), None, false, w, Turn::Ground, true);
    for &o in &cell.underlays {
        b.tile(at, o, (0.0, 0.0), None, false, w, Turn::Ground, false);
    }
    if let Some(f) = cell.feature_tile {
        b.tile(at, f, (0.0, 0.0), None, false, w, if cell.freestanding { Turn::Upright } else { Turn::Lean }, false);
    }
    if cell.kind == CellKind::Door {
        for &o in &cell.wall_overlays {
            b.tile(at, o, (0.0, 0.0), None, false, w, Turn::Ground, false);
        }
    }
    for &o in &cell.overlays {
        b.tile(at, o, (0.0, 0.0), None, false, w, Turn::Ground, false);
    }
}

/// What changes without a new scene, drawn over the list every frame: the player's minibars,
/// the cursor's grid and the cursor (render-2d `render`'s last steps).
pub fn build_marks(scene: &Scene, tiles: &mut MapTiles, cs: f32, style: &Style, minibars: Option<&Minibars>, cursor: Option<&SceneCursor>) -> DrawList {
    let blocks = style.mode == Mode::Blocks || !tiles.has_source();
    let mut b = Builder::new(tiles, cs);
    if scene.player_on_level && !blocks {
        // texel rows snapped to whole points, so the bars stay crisp at any cell size; edges
        // snapped rather than widths, so adjoining run segments neither gap nor overlap
        let s = cs / MINIBAR_CELL;
        let row = |t: f32| (t * s).round();
        for r in minibar_rects(minibars) {
            let top = row(r.y);
            let h = (row(r.y + r.h) - top).max(1.0);
            let left = row(r.x);
            let w = (row(r.x + r.w) - left).max(1.0);
            let c = r.colour;
            b.fill(scene.player, [left, top, w, h], palette::rgba((c >> 16) as u8, (c >> 8) as u8, c as u8, r.alpha), Turn::Upright, false);
        }
    }
    b.next_pass();
    if let Some(cursor) = cursor {
        if cursor.grid && !blocks {
            // a faint outline on every known cell: where the cursor may go
            let c = palette::rgba(255, 255, 255, 0.22);
            for cell in cells_in(scene, None) {
                if cell.kind != CellKind::Unknown {
                    outline(&mut b, (cell.x, cell.y), cs, 0.0, 1.0, c, Turn::Ground);
                }
            }
        }
        b.next_pass();
        // the same icon WebTiles paints over the cell; an outline when the gamedata has none
        let at = (cursor.x, cursor.y);
        let drawn = cursor.tile.is_some_and(|t| b.tile(at, t, (0.0, 0.0), None, false, Color32::WHITE, Turn::Lean, false));
        if !drawn {
            let c = if cursor.mode == isorun_scene::CursorMode::Map { Color32::WHITE } else { Color32::from_rgb(0xff, 0x88, 0x00) };
            outline(&mut b, at, cs, 0.0, 2.0, c, Turn::Lean);
        }
    }
    b.finish()
}

/// A square outline `width` thick inside the cell, `inset` from its edge (a canvas
/// `strokeRect` laid on the cell's pixels).
fn outline(b: &mut Builder, at: (i32, i32), cs: f32, inset: f32, width: f32, c: Color32, turn: Turn) {
    let (i, w) = (inset, width);
    let side = cs - 2.0 * i;
    b.fill(at, [i, i, side, w], c, turn, false);
    b.fill(at, [i, i + side - w, side, w], c, turn, false);
    b.fill(at, [i, i + w, w, side - 2.0 * w], c, turn, false);
    b.fill(at, [i + side - w, i + w, w, side - 2.0 * w], c, turn, false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use isorun_scene::{cell_key, Billboard, Rect, Visibility};

    fn scene(cells: &[(i32, i32, CellKind)]) -> Scene {
        let mut s = Scene { bounds: Rect { left: 0, top: 0, right: 9, bottom: 9 }, player_on_level: true, player: (1, 1), ..Default::default() };
        for &(x, y, kind) in cells {
            s.cells.insert(cell_key(x, y), SceneCell { x, y, kind, visibility: Visibility::Visible, floor_tile: 1, ..Default::default() });
        }
        s
    }

    #[test]
    fn cells_come_row_by_row() {
        let s = scene(&[(5, 1, CellKind::Floor), (0, 2, CellKind::Floor), (3, 1, CellKind::Wall), (2, 0, CellKind::Floor)]);
        let order: Vec<_> = cells_in(&s, None).iter().map(|c| (c.x, c.y)).collect();
        assert_eq!(order, vec![(2, 0), (3, 1), (5, 1), (0, 2)]);
        let order: Vec<_> = cells_in(&s, Some((3, 5, 1, 1))).iter().map(|c| (c.x, c.y)).collect();
        assert_eq!(order, vec![(3, 1), (5, 1)]);
    }

    #[test]
    fn blocks_without_tiles() {
        let mut s = scene(&[(0, 0, CellKind::Floor), (1, 0, CellKind::Wall), (1, 1, CellKind::Floor), (2, 2, CellKind::Unknown)]);
        let mut monster = Billboard::new(0, 0, 5, BillboardKind::Monster, 1.0);
        monster.attitude = Some(isorun_scene::Attitude::Hostile);
        s.billboards.push(monster);
        let mut tiles = MapTiles::default();
        let list = build(&s, &mut tiles, 10.0, &Style::default(), None);
        let c = MinimapColours::default();
        // three ground blocks (the unknown one is transparent), the monster's, then the player's last
        let colours: Vec<_> = list.items.iter().map(|i| i.colour).collect();
        assert_eq!(colours, vec![c.floor, c.wall, c.floor, c.monster, c.player]);
        assert_eq!(list.items[3].turn, Turn::Upright);
        assert_eq!(list.items[1].min, [10.0, 0.0]);
        assert!(list.items[0].base);
    }

    #[test]
    fn veils_over_remembered_cells() {
        let mut s = scene(&[(0, 0, CellKind::Floor)]);
        s.cells.get_mut(&cell_key(0, 0)).unwrap().visibility = Visibility::Remembered;
        let mut tiles = MapTiles::default();
        // no tiles: the cell's block and the player's, and no veil (the minimap's blocks carry
        // their own remembered colour)
        let list = build(&s, &mut tiles, 10.0, &Style::default(), None);
        let c = MinimapColours::default();
        let colours: Vec<_> = list.items.iter().map(|i| i.colour).collect();
        assert_eq!(colours, vec![c.remembered, c.player]);
    }

    #[test]
    fn cursor_outline_without_an_icon() {
        let s = scene(&[(0, 0, CellKind::Floor)]);
        let mut tiles = MapTiles::default();
        let style = Style { mode: Mode::Tiles, ..Default::default() };
        let cursor = SceneCursor { x: 0, y: 0, mode: isorun_scene::CursorMode::Map, tile: None, grid: false };
        let list = build_marks(&s, &mut tiles, 32.0, &style, None, Some(&cursor));
        assert_eq!(list.items.len(), 4);
        assert!(list.items.iter().all(|i| i.colour == Color32::WHITE && i.turn == Turn::Lean));
        assert_eq!(list.items[0].min, [0.0, 0.0]);
        assert_eq!(list.items[1].max, [32.0, 32.0]);
    }

    #[test]
    fn grouped_by_stack_then_atlas_spills_last() {
        let mut tiles = MapTiles::default();
        let mut b = Builder::new(&mut tiles, 10.0);
        let w = Color32::WHITE;
        let quad = |tex, min: [f32; 2], max: [f32; 2]| Item { tex, min, max, uv_min: [0.0; 2], uv_max: [0.0; 2], colour: w, turn: Turn::Ground, anchor: [0.0; 2], base: false };
        b.push((0, 0), quad(Tex::Atlas(1), [0.0, 0.0], [10.0, 10.0]));
        b.push((0, 0), quad(Tex::Atlas(0), [0.0, 0.0], [10.0, 10.0]));
        // a tall one, out over the cell above
        b.push((1, 0), quad(Tex::Atlas(0), [10.0, -10.0], [20.0, 10.0]));
        b.push((2, 0), quad(Tex::Atlas(0), [20.0, 0.0], [30.0, 10.0]));
        b.push((2, 0), quad(Tex::Atlas(1), [20.0, 0.0], [30.0, 10.0]));
        let list = b.finish();
        let got: Vec<_> = list.items.iter().map(|i| (i.tex, i.min[0] as i32)).collect();
        assert_eq!(got, vec![(Tex::Atlas(0), 20), (Tex::Atlas(1), 0), (Tex::Atlas(0), 0), (Tex::Atlas(1), 20), (Tex::Atlas(0), 10)]);
    }
}
