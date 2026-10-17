//! crawl's tiles on the HUD: an icon by name (crawl's command icons, `CMD_*`, in the GUI atlas;
//! the item tiles some rows borrow in main), the layers of an item or a spell as the server sends
//! them (overlays.ts `composeTiles`: each in its texture, cut at its `ymax`), the player's doll,
//! and the CSS `drop-shadow` the dock puts under them.

use crate::dock_actions::DockIcon;
use crate::quick_slots::TileLayer;
use egui::{Color32, Painter, Rect};
use isorun_scene::{Layer, TileId, TileRect};
use isorun_ui::tiles::TileIcons;
use isorun_webtiles::TileRef;

/// A tile to draw: where it is, and how it sits in its 32-texel cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub rect: TileRect,
    pub ox: f32,
    pub oy: f32,
}

/// The tile's id by its name: the GUI's, then the other tables (overlays.ts `commandTileId`).
pub fn named(tiles: &TileIcons, name: &str) -> Option<TileId> {
    let gd = &tiles.gamedata()?.gd;
    gd.gui.id(name).or_else(|| gd.main.id(name)).or_else(|| tiles.id_of(name))
}

/// The tiles of a dock icon.
pub fn icon_tiles(tiles: &TileIcons, icon: &DockIcon) -> Vec<Placed> {
    let id = match icon {
        DockIcon::Tile(t) => Some(*t),
        DockIcon::Gui(n) => tiles.gamedata().and_then(|g| g.gd.gui.id(n)),
        DockIcon::Dngn(n) => tiles.gamedata().and_then(|g| g.gd.dngn.id(n)),
    };
    id.and_then(|id| tiles.rect(id)).map(|rect| vec![Placed { rect, ox: 0.0, oy: 0.0 }]).unwrap_or_default()
}

/// The layers of an item, a spell or an ability as the server named them, cut at their `ymax`.
pub fn layer_tiles(tiles: &TileIcons, layers: &[TileLayer]) -> Vec<Placed> {
    layers
        .iter()
        .filter_map(|l| {
            let mut rect = match l.tex {
                Some(tex) => tiles.rect_of_ref(&TileRef { t: l.t, tex, ymax: None })?,
                None => tiles.rect(l.t as TileId)?,
            };
            if let Some(y) = l.ymax {
                let y = y as i32;
                if y < rect.oy + rect.h as i32 {
                    rect.h = (y - rect.oy).max(0) as u32;
                }
            }
            (rect.h > 0).then_some(Placed { rect, ox: 0.0, oy: 0.0 })
        })
        .collect()
}

/// The player's doll (screens.ts `playerSprite`): its parts bottom to top, or its one tile.
pub fn doll_tiles(tiles: &TileIcons, tile: TileId, layers: &[Layer]) -> Vec<Placed> {
    if layers.is_empty() {
        return tiles.rect(tile).map(|rect| vec![Placed { rect, ox: 0.0, oy: 0.0 }]).unwrap_or_default();
    }
    layers
        .iter()
        .filter_map(|l| {
            let mut rect = tiles.rect(l.tile)?;
            if let Some(y) = l.ymax {
                let y = y as i32;
                if y < rect.oy + rect.h as i32 {
                    rect.h = (y - rect.oy).max(0) as u32;
                }
            }
            (rect.h > 0).then_some(Placed { rect, ox: l.ox, oy: l.oy })
        })
        .collect()
}

/// The tiles drawn one over the other into `cell` (a square standing for crawl's 32-texel cell).
pub fn paint(tiles: &mut TileIcons, painter: &Painter, placed: &[Placed], cell: Rect, tint: Color32) {
    let k = cell.width() / 32.0;
    for p in placed {
        let mut r = p.rect.clone();
        r.ox += p.ox.round() as i32;
        r.oy += p.oy.round() as i32;
        let _ = k;
        tiles.paint(painter, &r, cell, tint);
    }
}

/// The tiles with a CSS `drop-shadow(0 <dy> <blur> <colour>)` under them: the same tiles in the
/// shadow's colour, moved down and spread a little for the blur.
pub fn paint_with_shadow(tiles: &mut TileIcons, painter: &Painter, placed: &[Placed], cell: Rect, tint: Color32, dy: f32, blur: f32, shadow: Color32) {
    if blur > 0.5 {
        let reach = blur / 2.0;
        for (dx, ddy) in [(reach, 0.0), (-reach, 0.0), (0.0, reach), (0.0, -reach)] {
            paint(tiles, painter, placed, cell.translate(egui::vec2(dx, dy + ddy)), shadow.gamma_multiply(0.3));
        }
    }
    paint(tiles, painter, placed, cell.translate(egui::vec2(0.0, dy)), shadow.gamma_multiply(if blur > 0.5 { 0.5 } else { 0.8 }));
    paint(tiles, painter, placed, cell, tint);
}

/// A shadow tint: the tile's shape in `c` (an egui tint multiplies the texel, so black is black
/// and the alpha stays the tile's).
pub fn silhouette(c: Color32) -> Color32 {
    c
}
