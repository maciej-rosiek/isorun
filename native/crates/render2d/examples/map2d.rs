//! The 2D views in the harness: an offline game with the minimap at the top right, and the
//! level map over the view while crawl's map mode is up (press `X`), as the game screen shows
//! them. `ISORUN_MAP2D_STATS=1` logs how long building and drawing took.

use isorun_render2d::{map_cell_size, minibars, LevelMap, MapTiles, Minimap};
use isorun_scene::{CursorMode, SceneCursor};
use isorun_webtiles::UiState;
use std::sync::Arc;

fn main() -> eframe::Result {
    let mut tiles = MapTiles::default();
    let mut level_map = LevelMap::new();
    let mut minimap = Minimap::new();
    // the HUD's layout says where the minimap's slot and the level map's view are
    let mut hud = isorun_hud::Hud::new();
    let settings = isorun_client::servers::get_settings();
    let mut installed = false;
    let stats = std::env::var("ISORUN_MAP2D_STATS").is_ok();
    isorun_ui::harness::run("Isorun map2d", move |ui, game| {
        if !game.playing() {
            return false;
        }
        let gd = game.session.gamedata.clone().map(|g| g as Arc<dyn isorun_render::TileSource>);
        tiles.set_source(gd);
        let st = &game.session.state;
        let scene = game.session.scene.clone();
        if !installed {
            installed = true;
            isorun_hud::install(ui.ctx());
        }
        let map_view = st.ui_state == UiState::VIEW_MAP;
        let lay = hud.layout(ui.ctx(), ui.max_rect(), st, &settings, map_view);
        // crawl's cursor (game.ts `cursorFor`): the mouse or targeting one, then the map's, then the tutorial's
        let cursor = [0usize, 2, 1].iter().find_map(|&i| st.cursors.get(i).copied().flatten()).map(|c| {
            let mode = if st.ui_state == UiState::VIEW_MAP { CursorMode::Map } else if st.input_mode == isorun_webtiles::MouseMode::COMMAND { CursorMode::Examine } else { CursorMode::Target };
            let tutorial = st.cursors.first().copied().flatten().is_none() && st.cursors.get(2).copied().flatten().is_none();
            let tile = game.session.gamedata.as_ref().and_then(|g| g.gd.icons.id(if tutorial { "TUTORIAL_CURSOR" } else { "CURSOR" }));
            SceneCursor { x: c.x, y: c.y, mode, tile, grid: false }
        });
        let t0 = std::time::Instant::now();
        if map_view {
            // the level map keeps to the clear part of the view, over the sidebar and the
            // messages when the rc's tile_level_map_hide_* say so (grid/console.ts `levelMapSplit`)
            let rect = lay.view;
            level_map.set_options(&st.options);
            let p = &st.player;
            level_map.minibars = Some(minibars(&st.options, p.hp, p.hp_max, p.mp, p.mp_max));
            let cell = map_cell_size(&st.options, rect.width(), rect.height());
            let centre = st.map.view_center.map(|c| (c.x, c.y)).unwrap_or(scene.player);
            // the native engine has sent no `cursor` message in these runs; the map cursor is the
            // view centre (game.ts: vgrdc is the map cursor while the map is open), so the
            // harness shows it there to compare with the web build
            let cursor = cursor.or_else(|| {
                let tile = game.session.gamedata.as_ref().and_then(|g| g.gd.icons.id("CURSOR"));
                Some(SceneCursor { x: centre.0, y: centre.1, mode: CursorMode::Map, tile, grid: false })
            });
            let r = level_map.ui(ui, rect, &scene, &mut tiles, cell, centre, cursor.as_ref());
            if let Some((c, b)) = r.clicked {
                log::info!("clicked {c:?} with {b:?}");
            }
            if stats {
                let (meshes, vertices) = level_map.stats();
                log::info!("level map {:.2} ms, {meshes} meshes, {vertices} vertices", t0.elapsed().as_secs_f64() * 1e3);
            }
        } else {
            // the minimap's slot: hung off the right column's top-right corner (hud.ts `layout`)
            let rect = lay.minimap;
            minimap.cell = settings.minimap_cell as f32;
            minimap.set_options(&st.options);
            let cam = game.camera.camera();
            let centre = (cam.x, cam.y);
            minimap.upright_yaw = Some(cam.yaw as f32);
            let r = minimap.ui(ui, rect, &scene, &mut tiles, centre, cam.yaw as f32, cursor.as_ref());
            if r.clicked() {
                // the game opens the level map (crawl's `X`)
                game.send(isorun_webtiles::key_message('X' as i32));
            }
            if stats {
                log::info!("minimap {:.2} ms", t0.elapsed().as_secs_f64() * 1e3);
            }
        }
        false
    })
}
