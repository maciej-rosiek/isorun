//! A WebTiles `GameState` plus the server's `Gamedata`, turned into the renderer-facing
//! `Scene` (the port of `packages/scene-webtiles`). Every piece of DCSS knowledge rendering
//! needs is spent here: tile ranges, tile names, glyphs, map-feature categories, body sizes,
//! branches. Renderers only ever see the Scene.

mod build;
mod classify;
mod enums;
mod monsters;
mod viewmodel;

pub use build::{build_scene, missing_tile_names, BuildOptions, BuildRecord, Dirty, CEILING_REACH, ITEM_HEIGHT, LYING_LENGTH};
pub use classify::{
    beacon_for, classify_feature, classify_feature_with, feature_label, gate_faces, is_scenery, is_vegetation, item_tile_name, level_presentation,
    liquid_label, liquid_of, shop_label, stance_for, stands_free, LevelPresentation,
};
pub use enums::{enum_mismatches, server_enums, HaloTable, MfTable, ServerEnums, HALO_DEFAULT};
pub use monsters::{attitude_of, is_excluded_from_list, monster_groups, monster_of, monster_sort, threat_of, visible_monsters};
pub use viewmodel::viewmodel_for;
