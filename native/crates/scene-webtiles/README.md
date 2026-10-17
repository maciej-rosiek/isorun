# isorun-scene-webtiles

Turns an `isorun-webtiles` `GameState` plus the server's `isorun-gamedata`
`Gamedata` into an `isorun-scene` `Scene`; the port of
`packages/scene-webtiles`. This is where all of Isorun's knowledge of DCSS
lives: which tile ranges are walls, floors, doors, stairs, water and lava; how
a cell's flags become cursors, halos and status icons; which monsters the
monster list shows, in what order, grouped how; what the player is holding.

- `build_scene(&state, &gd, BuildOptions { previous, dirty, record })`: the
  whole thing. Given the previous scene, the reducer's dirty cells
  (`Dirty::of(&state)`, then `state.clear_dirty()`) and the `BuildRecord` of
  the previous build, only the touched cells (and what depends on them: the
  lids near a changed wall, the player's old and new cells) are built again;
  every other cell and billboard is copied from the previous scene. A map
  clear, another state or gamedata, a change of level or demon-tier option, or
  a record that does not describe `previous` builds everything.
  `BuildRecord::carried()` / `rebuilt()` say what the last build did.
  `layout_revision` moves only when something a level mesh is built from
  changed (`isorun_scene::scene_layout_equals`).
- `classify_feature` (`classify_feature_with` for a server's own `MfTable`),
  `stance_for`, `stands_free`, `beacon_for`, `feature_label`, `shop_label`,
  `gate_faces`, `level_presentation`, `liquid_of`, `liquid_label`,
  `item_tile_name`, `is_vegetation`, `is_scenery`: the classification rules,
  exported so a renderer or HUD can ask the same questions.
- `is_excluded_from_list`, `monster_sort`, `visible_monsters`,
  `monster_groups`: the official client's monster list rules. The TS reads a
  monster off its billboard's `ref`; here `monster_of(&state, billboard)` finds
  it by `ref_id` (crawl's `mon.id`) or by its cell.
- `viewmodel_for(&state, Some(&gd))`: the wielded weapon, off-hand and shield.
- `missing_tile_names(&gd)`: which names this crate draws by that a server's
  tileinfo lacks; `server_enums(&gd)` / `enum_mismatches(&gd)`: the `MF_*` and
  `HALO_*` values in use, and every hardcoded constant (`MouseMode`,
  `MenuFlag`, `UiState`, `MapFeature`, the texture order) that the server's
  `enums.js` disagrees with. Both are for a diagnostic at connect time; the
  caller logs them.

Monster billboards carry `ref_id` = `mon.id`; the player's carries
`PLAYER_REF`. Cells are built in key order (row by row), so a build does not
depend on hash order; a tie for the level's lid goes to the lower tile id.

Tested against a recorded session and the checked-in tileinfo of one gamedata
version (`native/fixtures`): `cargo test -p
isorun-scene-webtiles`. Part of [Isorun](https://github.com/maciej-rosiek/isorun).
AGPL-3.0-or-later; see the repository's ATTRIBUTION.md for what follows the
official client's `cell_renderer.js` and `monster_list.js`.
