//! Crawl's own 2D tile view on egui (the port of `packages/render-2d` and of the minimap in
//! `apps/isorun/src/hud.ts`): the level map that `X` opens over the isometric view
//! ([`LevelMap`]) and the minimap at the top right of the game screen ([`Minimap`]).
//!
//! A scene is turned into a draw list once per revision ([`build`]), in the official client's
//! layer order, and placed on the screen as one egui mesh per run of one atlas ([`emit`]), so
//! a whole level is a handful of draw calls and a frame that changes nothing rebuilds nothing.

pub mod atlas;
pub mod build;
pub mod emit;
pub mod layout;
pub mod level_map;
pub mod minimap;
pub mod palette;

pub use atlas::MapTiles;
pub use build::{Mode, Style};
pub use layout::{cell_pixels, map_cell_size, minimap_box, StickPan, MINIMAP_CELLS, MINIMAP_CELL_DEFAULT, MINIMAP_TILES_DEFAULT, MINIMAP_TILES_MAX, MINIMAP_TILES_MIN};
pub use level_map::{LevelMap, MapResponse};
pub use minimap::Minimap;
pub use palette::MinimapColours;

/// The player's mini bars as the rc and the `player` message say (game.ts
/// `applyServerOptions`: `tile_show_minihealthbar`, `tile_show_minimagicbar`, on unless the rc
/// turns them off).
pub fn minibars(options: &serde_json::Map<String, serde_json::Value>, hp: i64, hp_max: i64, mp: i64, mp_max: i64) -> isorun_scene::Minibars {
    let on = |k: &str| options.get(k).and_then(|v| v.as_bool()) != Some(false);
    isorun_scene::Minibars { hp, hp_max, mp, mp_max, show_hp: on("tile_show_minihealthbar"), show_mp: on("tile_show_minimagicbar") }
}
