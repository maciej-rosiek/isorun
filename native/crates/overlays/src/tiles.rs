//! Tiles in the overlays (overlays.ts `tileCanvas`, `composeTiles`, `monsterTiles`): a stack of
//! tile layers drawn one over the other in a square of `size` CSS px that stands for crawl's
//! 32x32 cell, each layer at its own offset and cut at its `ymax`.

use egui::{Color32, Pos2, Rect, Vec2};
use isorun_ui::tiles::TileIcons;
use serde_json::{Map, Value};

/// One layer: the tile by the client's id (`tex` None) or by the server's texture and index.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Tile {
    pub t: i64,
    pub tex: Option<i64>,
    pub ymax: Option<f64>,
    pub ox: i32,
    pub oy: i32,
}

impl Tile {
    pub fn id(t: i64) -> Self {
        Tile { t, ..Default::default() }
    }
    /// A `{t, tex, ymax}` the server sent.
    pub fn from_json(v: &Value) -> Option<Tile> {
        let o = v.as_object()?;
        Some(Tile { t: o.get("t")?.as_i64()?, tex: o.get("tex").and_then(|x| x.as_i64()), ymax: o.get("ymax").and_then(|x| x.as_f64()), ox: 0, oy: 0 })
    }
    /// A list of them, or one alone.
    pub fn list(v: Option<&Value>) -> Vec<Tile> {
        match v {
            Some(Value::Array(a)) => a.iter().filter_map(Tile::from_json).collect(),
            Some(o @ Value::Object(_)) => Tile::from_json(o).into_iter().collect(),
            _ => Vec::new(),
        }
    }
}

impl From<&isorun_webtiles::TileRef> for Tile {
    fn from(r: &isorun_webtiles::TileRef) -> Self {
        Tile { t: r.t, tex: Some(r.tex), ymax: r.ymax, ox: 0, oy: 0 }
    }
}

/// Paint the stack into the square `cell`.
pub fn paint(icons: &mut TileIcons, painter: &egui::Painter, tiles: &[Tile], cell: Rect) {
    let k = cell.width() / 32.0;
    for t in tiles {
        let rect = match t.tex {
            Some(tex) => icons.rect_of_ref(&isorun_webtiles::TileRef { t: t.t, tex, ymax: None }),
            None => icons.rect(t.t as u32),
        };
        let Some(mut rect) = rect else { continue };
        if let Some(ymax) = t.ymax {
            let top = rect.oy as f64;
            if ymax < top + rect.h as f64 {
                let hh = (ymax - top).max(0.0);
                if hh <= 0.0 {
                    continue;
                }
                rect.h = hh as u32;
            }
        }
        rect.ox += t.ox;
        rect.oy += t.oy;
        // TileIcons draws the tile in a cell that is `rect.cell` texels across; ours is 32
        let unit = cell.width() / 32.0 * rect.cell.max(1) as f32 / 32.0;
        let _ = unit;
        let sq = Rect::from_min_size(cell.min, Vec2::splat(32.0 * k * rect.cell.max(1) as f32 / 32.0));
        icons.paint(painter, &rect, sq, Color32::WHITE);
    }
}

/// The tiles a monster describe popup draws in its header (ui-layouts.js describe_monster): a
/// monster is named by its foreground tile id; one built from a player doll or an mcache is the
/// stack of its parts, each at the offset its mcache entry gives it. A part taller than a cell
/// hangs below the top, as draw_dolls does.
pub fn monster_tiles(icons: &TileIcons, d: &Map<String, Value>) -> Vec<Tile> {
    let Some(gd) = icons.gamedata().map(|g| g.clone()) else { return Vec::new() };
    let gd = &gd.gd;
    let fg = d.get("fg_idx").and_then(|v| v.as_i64()).map(|id| gd.server_tile(id as u32, "main") as i64).unwrap_or(0);
    let player = |id: i64| gd.server_tile(id as u32, "player") as i64;
    let doll: Vec<(i64, i64)> = d
        .get("doll")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|x| Some((player(x.get(0)?.as_i64()?), x.get(1).and_then(|y| y.as_i64()).unwrap_or(32))))
        .collect();
    let mcache: Vec<(i64, i64, i64)> = d
        .get("mcache")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|x| Some((player(x.get(0)?.as_i64()?), x.get(1)?.as_i64().unwrap_or(0), x.get(2)?.as_i64().unwrap_or(0))))
        .collect();
    let r = &gd.ranges;
    let drop = |id: i64| (gd.tile(id as u32, None).map_or(32, |t| t.h as i64) - 32).max(0) as i32;
    if fg >= r.main_max && !doll.is_empty() {
        let base = drop(doll[0].0);
        return doll
            .iter()
            .map(|&(t, ymax)| {
                let m = mcache.iter().find(|m| m.0 == t);
                Tile { t, tex: None, ymax: Some(ymax as f64), ox: m.map_or(0, |m| m.1 as i32), oy: base + m.map_or(0, |m| m.2 as i32) }
            })
            .collect();
    }
    if fg >= r.mcache_start && !mcache.is_empty() {
        return mcache.iter().map(|&(t, ox, oy)| Tile { t, tex: None, ymax: None, ox: ox as i32, oy: oy as i32 + drop(t) }).collect();
    }
    if fg > 0 {
        vec![Tile::id(fg)]
    } else {
        Vec::new()
    }
}

/// A player doll the new-game screens draw (`[[id, ymax], ...]` in the player texture).
pub fn doll(v: Option<&Value>) -> Vec<Tile> {
    v.and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|x| Some(Tile { t: x.get(0)?.as_i64()?, tex: Some(3), ymax: x.get(1).and_then(|y| y.as_f64()), ox: 0, oy: 0 }))
        .collect()
}

/// The square a tile icon of `size` stands in, its left edge at `x`, centred on `cy`.
pub fn square(x: f32, cy: f32, size: f32) -> Rect {
    Rect::from_min_size(Pos2::new(x, (cy - size / 2.0).round()), Vec2::splat(size))
}
