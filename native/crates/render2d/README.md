# isorun-render2d

Crawl's own 2D tile view on egui: the port of `packages/render-2d` (`Render2d`) and of the
minimap in `apps/isorun/src/hud.ts`. Two widgets for the game screen:

- `LevelMap`: the level map that `X` opens over the isometric view (game.ts
  `syncMapRenderer`), crawl's 2D view of the whole level, black behind it, centred on the
  server's view centre (`vgrdc`, the map cursor while the map is open), with the cursor.
- `Minimap`: the small map at the top right of the game screen, following the player at the
  "Minimap tile size" setting's cell, turning with the view (`upYaw`), cut to the disc in its
  square with an antialiased rim, a soft drop shadow (styles.css `canvas.minimap`) and the
  north mark on its rim.

Depends on `isorun-scene` (the `Scene` it draws) and `isorun-render` (the `TileSource` tile
contract that the gamedata implements), and egui. AGPL-3.0-or-later.

## How it draws

A scene is turned into a draw list once (`build::build`): the cells in the official client's
layer order (terrain, what is built in, billboards, status badges, cell badges and the veils
over cells out of sight, the flash), each a quad in the map's own points with how it goes
with a turned map (`Turn`: the ground turns; monsters, items, the player and freestanding
features stay upright; built-in features and the cursor lean back as far as `upright_yaw`
says). Quads inside their own cell are grouped by their place in the cell's stack and then by
atlas, which changes no pixel (two cells' squares never overlap); what spills out of its cell
keeps the level's order after its pass. `emit::emit` places the list on the screen (a move for
the level map; a turn about the player for the minimap), culls it to the view, cuts it to the
disc, and batches it into one egui `Mesh` per run of one texture: a whole 80 x 70 level is a
handful of draw calls.

Both widgets keep what they built: the level map builds its list again only for a new scene
revision, cell size, style or tile source, and places it again only when it scrolls or the
view changes size, so a frame that changes nothing costs nothing but egui's own tessellation.
The minimap builds only the cells its disc can show (`layout::visible_range`), again for each
new revision, step and heading (every frame of a turn, a few hundred cells).

The player's mini bars and the cursor (`build::build_marks`) change without a new scene and
are kept apart from the level's list.

## API

```rust
use isorun_render2d::{map_cell_size, minibars, minimap_box, LevelMap, MapTiles, Minimap};

// one of each for the game screen
let mut tiles = MapTiles::default();          // the atlases as egui textures, for both views (shared with the icons: `isorun_ui::tiles::shared_atlas`)
let mut level_map = LevelMap::new();
let mut minimap = Minimap::new();

// each frame
tiles.set_source(session.gamedata.clone().map(|g| g as Arc<dyn TileSource>));
tiles.set_filter(options tile_filter_scaling == true);

// while crawl's map mode is up (ui_state VIEW_MAP): in the HUD's `HudLayout.view` laid out
// with map_view (grid/console.ts `levelMapSplit`: the clear part of the view, over the
// sidebar and the messages when the rc's tile_level_map_hide_* say so)
let rect = hud_layout.view;
level_map.set_options(&state.options);                         // tile_display_mode, tile_*_col
level_map.minibars = Some(minibars(&state.options, hp, hp_max, mp, mp_max));
let cell = map_cell_size(&state.options, rect.width(), rect.height()); // tile_cell_pixels * tile_map_scale, fitted
let centre = state.map.view_center.map(|c| (c.x, c.y)).unwrap_or(camera_cell);
let r = level_map.ui(ui, rect, &scene, &mut tiles, cell, centre, cursor.as_ref());
// r.hovered: the cell under the pointer; r.clicked: (cell, button), to send as crawl's click
// level_map.pick(&scene, pos): the cell under any point (game.ts `onPointer`)

// otherwise: the minimap, in the slot the HUD lays out for it (isorun-hud `HudLayout.minimap`,
// the square hud.ts `minimapBox` gives, its right edge on the right column's; Rect::NOTHING
// when there is no sidebar: draw none). `layout::minimap_box` is the same arithmetic.
let rect = hud_layout.minimap;
minimap.cell = settings.minimap_cell as f32;                  // "Minimap tile size"
minimap.set_options(&state.options);
minimap.upright_yaw = Some(camera.yaw as f32);                // hud.ts minimapUpright
let r = minimap.ui(ui, rect, &scene, &mut tiles, (camera.x, camera.y), camera.yaw as f32, server_cursor.as_ref());
if r.clicked() { /* open the level map (LEVEL_MAP) */ }
```

The cursor is the game's (`SceneCursor`, game.ts `cursorFor`): its `tile` is the gamedata's
`CURSOR` icon (`TUTORIAL_CURSOR` for the tutorial's), drawn as WebTiles paints it; without one
an outline, white for the map cursor and orange otherwise. The minimap shows only a cursor the
server placed, not the pointer's own hover cell. `cursor.grid` draws the faint outline on every
known cell.

`layout` has the pure arithmetic the game screen needs around it: `minimap_box` and the
"Minimap size" / "Minimap tile size" setting stops (`MINIMAP_TILES_MIN..MAX`,
`MINIMAP_CELLS`), `cell_pixels` (dungeon_renderer.js `fit_to`), `view_origin` and `pick`,
`north_mark`, and `StickPan` (game.ts `panMap`: the right stick over the level map steps the
map cursor once a whole cell's worth of deflection builds up; send the step as a cursor move).

The display modes: `Mode::Tiles` (the default), `Glyphs` and `Hybrid` (`tile_display_mode`),
and `Blocks`, the official minimap's colour blocks in its palette (`MinimapColours`, from the
rc's `tile_*_col` and the cells' map features), which is also what either view draws until
the tiles are in.

## See it

    cargo build -p isorun-render2d --example map2d
    ./target/debug/examples/map2d          # under Xvfb, see native/PORTING.md

An offline game with the minimap at the top right; `X` opens crawl's level map and the
level map covers the view while it is up. `ISORUN_MAP2D_STATS=1` logs each frame's time and
the level map's meshes and vertices.

    cargo test -p isorun-render2d
    cargo test -p isorun-render2d --test whole_level -- --ignored --nocapture

The second times a whole 80 x 70 level through the level map at 1600 x 1000: about 2 ms for a
new scene (the list built and placed), 0.3 ms for a step of the map's centre (placed again),
0.05 ms for a frame that changes nothing, egui's tessellation included; one mesh, one draw.

## Not yet

- In the harness the native engine sent no `cursor` messages (the state's `cursors` stay empty,
  and the map's `vgrdc` does not follow the map cursor's keys), so the example draws the map
  cursor on the view centre, where crawl puts it when the map opens. That is the engine's or the
  state's to look into, not this crate's: given a cursor, both views draw it.

- The facing cone (render-2d `facing`) is not drawn: the game passes none to either view.
- The north mark's halo is four offset copies rather than a blur.
