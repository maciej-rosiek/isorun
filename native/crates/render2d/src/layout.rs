//! The pure arithmetic of the 2D views: where the map's origin cell is, which cell a point
//! picks, which cells can show, the cell size the rc options ask for, the minimap's box and its
//! north mark, and the right stick's panning. No egui here, so all of it is unit-tested.

use isorun_scene::Rect;
use std::f32::consts::FRAC_PI_2;

/// game.js `show_diameter`: the field of view a full view must hold (dungeon_renderer.js `fit_to`).
pub const SHOW_DIAMETER: f32 = 17.0;

/// The "Minimap tile size" setting's default, in points: the cell the map follows the player at,
/// the same on any screen and under any UI scale, so the "Minimap size" setting can be a count
/// of tiles and mean exactly that.
pub const MINIMAP_CELL_DEFAULT: u32 = 20;
/// The "Minimap size" setting's default, in tiles across: enough of the level around the player
/// to read at a glance, and at `MINIMAP_CELL_DEFAULT` a map 380 points wide, a little past the
/// sidebar column of the reference layout (42 cells of 8 px, game.js `stat_width`, 336 px), so
/// it hangs over the view.
pub const MINIMAP_TILES_DEFAULT: u32 = 19;
/// The "Minimap size" setting's stops (settings-rows.ts): every odd count from 9 to 61, the
/// player on the centre tile.
pub const MINIMAP_TILES_MIN: u32 = 9;
pub const MINIMAP_TILES_MAX: u32 = 61;
/// The "Minimap tile size" setting's stops, in points: down to 8, near the 4 px cell WebTiles'
/// own auto-sized minimap comes out at, up to 32, `tile_cell_pixels`, a full dungeon tile.
pub const MINIMAP_CELLS: [u32; 9] = [8, 10, 12, 14, 16, 20, 24, 28, 32];
/// The most the "Minimap size" setting can take: half the screen across (the view keeps its
/// left half clear) and this much of the column down, so the monster list under it keeps some
/// rows even at the biggest stop. The disc is square, so whichever cap is tighter sets its size.
const MINIMAP_WIDEST: f32 = 0.5;
const MINIMAP_TALLEST: f32 = 0.7;

/// The odd count within one of `n` (odd counts keep the player on the centre tile), never under 1.
fn odd_up(n: f32) -> u32 {
    2 * (n.max(0.0) / 2.0).floor() as u32 + 1
}
/// The biggest odd count not over `n`, never under 1.
fn odd_down(n: f32) -> u32 {
    (2 * (n.max(1.0) / 2.0).ceil() as i64 - 1).max(1) as u32
}

/// The minimap's box (a square, `(w, h)` in points) on a sidebar column `side_w` x `side_h` on a
/// `grid_w` wide screen, at the "Minimap size" setting `tiles` and the "Minimap tile size"
/// setting `cell` (hud.ts `minimapBox`): that many `cell` point tiles on a side, taken to an odd
/// count so the player stands on the centre tile. The map is cut to the disc inscribed in it, so
/// the setting reads as the diameter: the map turns under the player, and a disc is the only
/// shape that shows the same reach on every heading.
///
/// It hangs off the column's right edge, so more tiles reach left over the view and down the
/// column, to half the screen and 0.7 of the column; a count that would pass either cap is cut
/// to the most whole tiles under both. Tiles say how much of the level shows, the cell how big
/// it is drawn: a bigger cell on the same count is the same map, larger, until a cap cuts it.
pub fn minimap_box(side_w: f32, side_h: f32, grid_w: f32, tiles: u32, cell: u32) -> (f32, f32) {
    if side_w <= 0.0 || side_h <= 0.0 {
        return (0.0, 0.0);
    }
    let across = if tiles >= 1 { tiles } else { MINIMAP_TILES_DEFAULT };
    let px = if cell >= 1 { cell } else { MINIMAP_CELL_DEFAULT } as f32;
    let side = odd_up(across as f32).min(odd_down((grid_w * MINIMAP_WIDEST).floor() / px)).min(odd_down((side_h * MINIMAP_TALLEST).floor() / px));
    (side as f32 * px, side as f32 * px)
}

/// How much of the map's radius the north tick takes, and its floor and ceiling in points.
const NORTH_TICK: (f32, f32, f32) = (0.045, 3.0, 8.0);
/// The N's type size as a fraction of the map's diameter, and its floor and ceiling in points.
const NORTH_TEXT: (f32, f32, f32) = (0.07, 8.0, 14.0);
/// How solid the north mark is drawn against the map under it.
pub const NORTH_ALPHA: f32 = 0.6;

/// Where the north mark sits on a `size` point minimap whose ground stands at `up_yaw` radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NorthMark {
    /// The centre the N is drawn about, from the map's top-left corner.
    pub x: f32,
    pub y: f32,
    /// The tick's inner and rim ends.
    pub tick: [(f32, f32); 2],
    /// The N's type size.
    pub font: f32,
}

/// hud.ts `northMark`. The ground turns by `-up_yaw` about the map's centre, so the level's
/// north (screen up on an unturned map) leaves the centre along (-sin, -cos): the mark rides
/// that heading round the rim rather than sitting at a fixed notch, as a mark that turned with
/// the map would say nothing the ground did not.
pub fn north_mark(size: f32, up_yaw: f32) -> NorthMark {
    let r = size / 2.0;
    let tick = (size * NORTH_TICK.0).clamp(NORTH_TICK.1, NORTH_TICK.2);
    let font = (size * NORTH_TEXT.0).clamp(NORTH_TEXT.1, NORTH_TEXT.2);
    let (dx, dy) = (-up_yaw.sin(), -up_yaw.cos());
    // the tick stops a hairline short of the rim, so its stroke lies inside the disc
    let outer = r - 1.0;
    let inner = outer - tick;
    // the letter stands clear of the tick by a little under half its own height
    let text = inner - font * 0.6;
    NorthMark { x: r + dx * text, y: r + dy * text, tick: [(r + dx * inner, r + dy * inner), (r + dx * outer, r + dy * outer)], font }
}

/// The cell at the view's top-left corner (render-2d `render`): on `centre` when one is given
/// (dungeon_renderer.js `set_view_center`: view.x = centre - floor(cols / 2), the server's view
/// centre, which is the map cursor while the level map is open), else the level's bounds
/// centred in the view.
pub fn view_origin(width: f32, height: f32, cs: f32, centre: Option<(i32, i32)>, bounds: Rect) -> (i32, i32) {
    let cols = (width / cs).ceil() as i32;
    let rows = (height / cs).ceil() as i32;
    match centre {
        Some((x, y)) => (x - cols.div_euclid(2), y - rows.div_euclid(2)),
        None => {
            let bw = bounds.right - bounds.left + 1;
            let bh = bounds.bottom - bounds.top + 1;
            (bounds.left - (cols - bw).div_euclid(2), bounds.top - (rows - bh).div_euclid(2))
        }
    }
}

/// The cell under a point `(px, py)` of an unturned map, from its top-left corner.
pub fn pick(px: f32, py: f32, cs: f32, origin: (i32, i32)) -> (i32, i32) {
    ((px / cs).floor() as i32 + origin.0, (py / cs).floor() as i32 + origin.1)
}

/// dungeon_renderer.js `fit_to` (game.ts `cellPixels`): the cell from the rc options, shrunk to
/// fit if a full field of view would not otherwise stand in the view. Glyph modes size their
/// cells from the font instead, so they are left alone.
pub fn cell_pixels(cell: f32, view_w: f32, view_h: f32, glyphs: bool) -> f32 {
    if glyphs || view_w <= 0.0 || view_h <= 0.0 {
        return cell;
    }
    let span = SHOW_DIAMETER * cell;
    if span <= view_w && span <= view_h {
        return cell;
    }
    (cell * (view_w / span).min(view_h / span)).floor().max(1.0)
}

/// The level map's cell (game.ts `applyServerOptions`): `tile_cell_pixels` (32) at
/// `tile_map_scale` percent (60), never under 8, fitted to the view by `cell_pixels`.
pub fn map_cell_size(options: &serde_json::Map<String, serde_json::Value>, view_w: f32, view_h: f32) -> f32 {
    let num = |k: &str, d: f64| options.get(k).and_then(|v| v.as_f64()).filter(|v| *v > 0.0).unwrap_or(d);
    let px = num("tile_cell_pixels", 32.0);
    let scale = num("tile_map_scale", 60.0);
    let glyphs = options.get("tile_display_mode").and_then(|v| v.as_str()) == Some("glyphs");
    cell_pixels(((px * scale) / 100.0).round().max(8.0) as f32, view_w, view_h, glyphs)
}

/// Where a cell's top-left corner `(sx, sy)` lands once the map is turned by `rot` about the
/// pivot `(pvx, pvy)` (both in the map's own points), and whether the cell is then in a
/// `width` x `height` view, with a cell's margin (render-2d `inView`).
pub fn in_view(sx: f32, sy: f32, cs: f32, width: f32, height: f32, rot: f32, pivot: (f32, f32)) -> bool {
    let (mut x, mut y) = (sx, sy);
    if rot != 0.0 {
        let (s, c) = rot.sin_cos();
        let dx = sx + cs / 2.0 - pivot.0;
        let dy = sy + cs / 2.0 - pivot.1;
        x = pivot.0 + dx * c - dy * s - cs / 2.0;
        y = pivot.1 + dx * s + dy * c - cs / 2.0;
    }
    !(x < -cs || y < -cs || x > width || y > height)
}

/// The cells a turned map can show, as an inclusive range `(x0, x1, y0, y1)` (render-2d
/// `minimapCells`): the view (with `in_view`'s margin) turned back onto the map. The map is
/// drawn again at every step of a turn, and a whole explored level is far more cells than the
/// disc holds, so the cells are looked up by position instead of walked.
pub fn visible_range(width: f32, height: f32, cs: f32, rot: f32, pivot: (f32, f32), origin: (i32, i32)) -> (i32, i32, i32, i32) {
    let (s, c) = rot.sin_cos();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (cx, cy) in [(-cs / 2.0, -cs / 2.0), (width + cs / 2.0, -cs / 2.0), (width + cs / 2.0, height + cs / 2.0), (-cs / 2.0, height + cs / 2.0)] {
        let dx = cx - pivot.0;
        let dy = cy - pivot.1;
        let x = pivot.0 + dx * c + dy * s;
        let y = pivot.1 - dx * s + dy * c;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    (
        (min_x / cs - 0.5).floor() as i32 - 1 + origin.0,
        (max_x / cs - 0.5).ceil() as i32 + 1 + origin.0,
        (min_y / cs - 0.5).floor() as i32 - 1 + origin.1,
        (max_y / cs - 0.5).ceil() as i32 + 1 + origin.1,
    )
}

/// How far past its square a cell's opaque ground is drawn, in points: nothing on a map turned
/// in whole quarters, one device pixel on a map turned off them, where every cell's edge is
/// antialiased on its own and two half-covered edges leave a seam of what lies behind.
pub fn seam(rot: f32, pixels_per_point: f32) -> f32 {
    if (((rot / FRAC_PI_2).round() * FRAC_PI_2) - rot).abs() > 1e-6 {
        1.0 / pixels_per_point.max(0.01)
    } else {
        0.0
    }
}

/// game.ts `MAP_PAN_RATE`: how many cells a full deflection of the right stick builds up a frame.
pub const MAP_PAN_RATE: f32 = 0.12;

/// The right stick over the level map (game.ts `panMap`): the deflection builds up, and each
/// time a whole cell's worth has, the map cursor steps that way (north-up, absolute).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StickPan {
    x: f32,
    y: f32,
}

impl StickPan {
    /// One frame of the stick at `(dx, dy)` (each -1..1); the step to send as a cursor move,
    /// `(sx, sy)` each -1, 0 or 1, when a whole cell has built up. A centred stick starts over.
    pub fn step(&mut self, dx: f32, dy: f32) -> Option<(i32, i32)> {
        if dx == 0.0 && dy == 0.0 {
            *self = StickPan::default();
            return None;
        }
        self.x += dx * MAP_PAN_RATE;
        self.y += dy * MAP_PAN_RATE;
        let sx = if self.x.abs() >= 1.0 { self.x.signum() } else { 0.0 };
        let sy = if self.y.abs() >= 1.0 { self.y.signum() } else { 0.0 };
        if sx == 0.0 && sy == 0.0 {
            return None;
        }
        self.x -= sx;
        self.y -= sy;
        Some((sx as i32, sy as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn minimap_box_default_is_19_tiles() {
        assert_eq!(minimap_box(336.0, 900.0, 1600.0, 19, 20), (380.0, 380.0));
        // an even count goes up to the next odd one
        assert_eq!(minimap_box(336.0, 900.0, 1600.0, 20, 20), (420.0, 420.0));
        assert_eq!(minimap_box(0.0, 900.0, 1600.0, 19, 20), (0.0, 0.0));
    }

    #[test]
    fn minimap_box_caps() {
        // half the screen across: 400 / 20 = 20 -> 19 tiles
        assert_eq!(minimap_box(336.0, 2000.0, 800.0, 61, 20), (380.0, 380.0));
        // 0.7 of the column down: 280 / 20 = 14 -> 13 tiles
        assert_eq!(minimap_box(336.0, 400.0, 1600.0, 61, 20), (260.0, 260.0));
        // never under one tile
        assert_eq!(minimap_box(336.0, 10.0, 1600.0, 19, 20), (20.0, 20.0));
        // a bad setting takes the defaults
        assert_eq!(minimap_box(336.0, 900.0, 1600.0, 0, 0), (380.0, 380.0));
    }

    #[test]
    fn north_mark_rides_the_rim() {
        let m = north_mark(380.0, 0.0);
        assert!((m.x - 190.0).abs() < 1e-4);
        assert!(m.y < 190.0);
        assert_eq!(m.tick[1], (190.0, 1.0));
        assert_eq!(m.font, 14.0);
        // the ground turned a quarter clockwise: north is to the left
        let m = north_mark(380.0, PI / 2.0);
        assert!(m.x < 30.0 && (m.y - 190.0).abs() < 1e-3);
        assert_eq!(north_mark(100.0, 0.0).font, 8.0);
    }

    #[test]
    fn origin_on_the_centre() {
        // 10 x 7 cells, the centre at 5, 3 of them
        assert_eq!(view_origin(320.0, 224.0, 32.0, Some((40, 20)), Rect::default()), (35, 17));
        // a part cell counts as a column
        assert_eq!(view_origin(330.0, 224.0, 32.0, Some((40, 20)), Rect::default()), (35, 17));
        // no centre: the level's bounds in the middle
        let b = Rect { left: 10, top: 5, right: 19, bottom: 9 };
        assert_eq!(view_origin(640.0, 320.0, 32.0, None, b), (5, 3));
    }

    #[test]
    fn picks_the_cell_under_the_point() {
        assert_eq!(pick(0.0, 0.0, 32.0, (35, 17)), (35, 17));
        assert_eq!(pick(33.0, 63.9, 32.0, (35, 17)), (36, 18));
        assert_eq!(pick(-1.0, 0.0, 32.0, (35, 17)), (34, 17));
    }

    #[test]
    fn cell_fits_the_field_of_view() {
        assert_eq!(cell_pixels(19.0, 1600.0, 1000.0, false), 19.0);
        // 17 cells of 32 in a 390 tall view
        assert_eq!(cell_pixels(32.0, 844.0, 390.0, false), 22.0);
        assert_eq!(cell_pixels(32.0, 844.0, 390.0, true), 32.0);
        let mut o = serde_json::Map::new();
        assert_eq!(map_cell_size(&o, 1600.0, 1000.0), 19.0);
        o.insert("tile_map_scale".into(), 100.into());
        assert_eq!(map_cell_size(&o, 1600.0, 1000.0), 32.0);
        o.insert("tile_map_scale".into(), 10.into());
        assert_eq!(map_cell_size(&o, 1600.0, 1000.0), 8.0);
    }

    #[test]
    fn culling() {
        assert!(in_view(0.0, 0.0, 20.0, 100.0, 100.0, 0.0, (50.0, 50.0)));
        assert!(in_view(-19.0, 0.0, 20.0, 100.0, 100.0, 0.0, (50.0, 50.0)));
        assert!(!in_view(-21.0, 0.0, 20.0, 100.0, 100.0, 0.0, (50.0, 50.0)));
        assert!(!in_view(101.0, 0.0, 20.0, 100.0, 100.0, 0.0, (50.0, 50.0)));
        // a half turn about the centre brings the far cell round to the near side
        assert!(in_view(140.0, 40.0, 20.0, 100.0, 100.0, PI, (50.0, 50.0)) == in_view(-60.0, 40.0, 20.0, 100.0, 100.0, 0.0, (50.0, 50.0)));
        // unturned: the view and a cell's margin either side
        assert_eq!(visible_range(100.0, 100.0, 20.0, 0.0, (50.0, 50.0), (0, 0)), (-2, 6, -2, 6));
        // turned 45 degrees the range widens to the view's turned corners
        let (x0, x1, _, _) = visible_range(100.0, 100.0, 20.0, PI / 4.0, (50.0, 50.0), (0, 0));
        assert!(x0 < -2 && x1 > 6);
    }

    #[test]
    fn seam_only_off_the_quarters() {
        assert_eq!(seam(0.0, 1.0), 0.0);
        assert_eq!(seam(PI / 2.0, 2.0), 0.0);
        assert_eq!(seam(0.3, 2.0), 0.5);
    }

    #[test]
    fn stick_steps_a_cell_at_a_time() {
        let mut p = StickPan::default();
        let steps: Vec<_> = (0..10).filter_map(|_| p.step(1.0, 0.0)).collect();
        assert_eq!(steps, vec![(1, 0)]);
        assert_eq!(p.step(0.0, 0.0), None);
        assert_eq!(p, StickPan::default());
        let steps: Vec<_> = (0..9).filter_map(|_| p.step(-1.0, -1.0)).collect();
        assert_eq!(steps, vec![(-1, -1)]);
    }
}
