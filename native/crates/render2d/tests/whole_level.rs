//! A whole level (80 x 70 cells) through the level map, timed: `cargo test -p isorun-render2d
//! --release --test whole_level -- --ignored --nocapture`. The colour blocks stand in for tiles
//! (no gamedata here); the quads and the meshes are the same work.

use isorun_render2d::{LevelMap, MapTiles};
use isorun_scene::{cell_key, Billboard, BillboardKind, CellKind, Rect, Scene, SceneCell, Visibility};
use std::time::Instant;

fn level() -> Scene {
    let mut s = Scene { bounds: Rect { left: 0, top: 0, right: 79, bottom: 69 }, player: (40, 35), player_on_level: true, revision: 1, ..Default::default() };
    for y in 0..70 {
        for x in 0..80 {
            let kind = if (x * 7 + y * 3) % 5 == 0 { CellKind::Wall } else { CellKind::Floor };
            let visibility = if (x - 40i32).abs() < 8 && (y - 35i32).abs() < 8 { Visibility::Visible } else { Visibility::Remembered };
            s.cells.insert(cell_key(x, y), SceneCell { x, y, kind, visibility, floor_tile: 1, ..Default::default() });
            if (x + y) % 13 == 0 {
                s.billboards.push(Billboard::new(x, y, 2, BillboardKind::Monster, 1.0));
            }
        }
    }
    s
}

#[test]
#[ignore]
fn whole_level_frames() {
    let ctx = egui::Context::default();
    let mut map = LevelMap::new();
    let mut tiles = MapTiles::default();
    let scene = level();
    let mut times = Vec::new();
    for frame in 0..40 {
        // a step of the map's centre every tenth frame (panning), a new scene every twentieth
        let mut s = scene.clone();
        s.revision = 1 + frame / 20;
        let centre = (40 + (frame / 10) as i32, 35);
        let t = Instant::now();
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))), ..Default::default() }, |ui| {
            map.ui(ui, egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0)), &s, &mut tiles, 32.0, centre, None);
        });
        let built = t.elapsed().as_secs_f64() * 1e3;
        // what egui does with it before the GPU: the meshes into its own primitives
        let prims = ctx.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
        out.textures_delta.clear();
        times.push((frame, built, t.elapsed().as_secs_f64() * 1e3, prims.len()));
    }
    let (meshes, vertices) = map.stats();
    println!("{meshes} meshes, {vertices} vertices");
    for (f, ms, total, prims) in times {
        println!("frame {f}: ui {ms:.2} ms, with tessellation {total:.2} ms, {prims} primitives");
    }
}
