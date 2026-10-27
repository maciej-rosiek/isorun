//! What the renderer draws, in world space, before any GPU work (the port of `plan.ts`): the
//! ground layers, the wall blocks with their faces, and what stands; the cut-away of the walls in
//! front of the player; screen-relative steering.

use isorun_scene::{cell_key, has_items_under, yaw_to_dir, Billboard, BillboardKind, CellKey, CellKind, Dir8, Scene, SceneCell, TileId, TileRect, Visibility};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsoView {
    /// Screen pixels per cell.
    pub zoom: f32,
    /// Wall height in cells.
    pub wall_height: f32,
    /// The field of view in degrees, top to bottom: 0 is the isometric view.
    pub fov: f32,
}

pub const DEFAULT_VIEW: IsoView = IsoView { zoom: 68.0, wall_height: 1.0, fov: 0.0 };

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AimIntent {
    Move,
    Attack,
    Blocked,
}

pub fn aim_colour(intent: AimIntent) -> u32 {
    match intent {
        AimIntent::Move => 0xffd75e,
        AimIntent::Attack => 0xff5a4a,
        AimIntent::Blocked => 0x9a9a9a,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsoAim {
    pub dir: Dir8,
    pub intent: AimIntent,
}

/// A way to a cell: the cells to step on before it, and what happens on it.
#[derive(Clone, Debug, PartialEq)]
pub struct IsoRoute {
    pub steps: Vec<(i32, i32)>,
    pub target: (i32, i32),
    pub intent: AimIntent,
}

/// Screen right and screen up as directions on the ground, for a yaw (0 = north is up the screen).
pub fn ground_basis(yaw: f32) -> ([f32; 2], [f32; 2]) {
    ([yaw.cos(), yaw.sin()], [yaw.sin(), -yaw.cos()])
}

/// The compass direction a screen-space vector (x right, y down) points at on the ground.
pub fn stick_to_dir(x: f32, y: f32, yaw: f32) -> Dir8 {
    let (right, forward) = ground_basis(yaw);
    let wx = x * right[0] - y * forward[0];
    let wy = x * right[1] - y * forward[1];
    yaw_to_dir(wx.atan2(-wy) as f64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Side {
    N,
    S,
    W,
    E,
}

pub const SIDE_NAMES: [Side; 4] = [Side::N, Side::S, Side::W, Side::E];

pub struct SideInfo {
    pub normal: (i32, i32),
    pub from: (i32, i32),
    pub to: (i32, i32),
}

/// Each side of a cell: its outward normal and its two ground corners, left to right as seen from outside.
pub fn side(s: Side) -> SideInfo {
    match s {
        Side::N => SideInfo { normal: (0, -1), from: (1, 0), to: (0, 0) },
        Side::S => SideInfo { normal: (0, 1), from: (0, 1), to: (1, 1) },
        Side::W => SideInfo { normal: (-1, 0), from: (0, 0), to: (0, 1) },
        Side::E => SideInfo { normal: (1, 0), from: (1, 1), to: (1, 0) },
    }
}

pub const CUTAWAY_HEIGHT: f32 = 0.2;
const PLAYER_HALF_WIDTH: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundLayer {
    pub x: i32,
    pub y: i32,
    pub tile: TileId,
    /// Draw order above the floor, 0 for the floor itself.
    pub lift: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallFace {
    pub side: Side,
    pub tile: TileId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WallBlock {
    pub x: i32,
    pub y: i32,
    pub height: f32,
    pub top: TileId,
    pub faces: Vec<WallFace>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteLayer {
    pub tile: TileId,
    pub ox: f32,
    pub oy: f32,
    pub ymax: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    /// Index into `scene.billboards`, or none for a feature that stands.
    pub billboard: Option<usize>,
    pub x: i32,
    pub y: i32,
    pub layers: Vec<SpriteLayer>,
    pub lying: bool,
    /// The top item of a pile: more items lie under it.
    pub pile: bool,
}

#[derive(Clone, Debug, Default)]
pub struct IsoPlan {
    pub ground: Vec<GroundLayer>,
    pub walls: Vec<WallBlock>,
    pub sprites: Vec<Sprite>,
}

/// The cells walls must not hide: the player's, every floor cell seen within `reach` of it, and one more.
pub fn cut_away_focus(scene: &Scene, extra: Option<(i32, i32)>, reach: i32) -> Vec<(i32, i32)> {
    let mut focus = Vec::new();
    if scene.player_on_level {
        let (px, py) = scene.player;
        focus.push(scene.player);
        for y in py - reach..=py + reach {
            for x in px - reach..=px + reach {
                if (x == px && y == py) || (((x - px).pow(2) + (y - py).pow(2)) as f32).sqrt() > reach as f32 {
                    continue;
                }
                if is_pointable(scene.cell(x, y)) {
                    focus.push((x, y));
                }
            }
        }
    }
    if let Some(e) = extra {
        focus.push(e);
    }
    focus
}

/// Whether the wall at a cell stands between the camera and a focus cell, so it drops to a stub.
pub fn is_cut_away(x: i32, y: i32, focus: (i32, i32), yaw: f32, elevation: f32, wall_height: f32) -> bool {
    let (right, forward) = ground_basis(yaw);
    let dx = (x - focus.0) as f32;
    let dy = (y - focus.1) as f32;
    let depth = dx * forward[0] + dy * forward[1];
    let across = dx * right[0] + dy * right[1];
    let half_across = (right[0].abs() + right[1].abs()) / 2.0;
    let half_depth = (forward[0].abs() + forward[1].abs()) / 2.0;
    let top = (depth + half_depth) * elevation.sin() + wall_height * elevation.cos();
    depth < 0.0 && across.abs() < half_across + PLAYER_HALF_WIDTH && top > 0.0
}

/// The height of the wall at a cell as drawn: a stub when it hides a focus cell.
pub fn wall_height_at(x: i32, y: i32, focus: &[(i32, i32)], yaw: f32, elevation: f32, view: &IsoView) -> f32 {
    if focus.iter().any(|&f| is_cut_away(x, y, f, yaw, elevation, view.wall_height)) {
        CUTAWAY_HEIGHT.min(view.wall_height)
    } else {
        view.wall_height
    }
}

/// The wall cells that drop to stubs for these focus cells.
pub fn cut_away_cells(scene: &Scene, focus: &[(i32, i32)], yaw: f32, elevation: f32, view: &IsoView) -> HashSet<CellKey> {
    let reach = (view.wall_height / elevation.tan().max(0.01)).ceil() as i32 + 2;
    let mut cut = HashSet::new();
    for &(fx, fy) in focus {
        for y in fy - reach..=fy + reach {
            for x in fx - reach..=fx + reach {
                if scene.cell(x, y).is_some_and(|c| c.occluder) && is_cut_away(x, y, (fx, fy), yaw, elevation, view.wall_height) {
                    cut.insert(cell_key(x, y));
                }
            }
        }
    }
    cut
}

/// The point where a view ray meets the ground, in cells.
pub fn ground_point(origin: [f32; 3], direction: [f32; 3]) -> Option<(f32, f32)> {
    if direction[1] >= 0.0 {
        return None;
    }
    let t = -origin[1] / direction[1];
    Some((origin[0] + direction[0] * t, origin[2] + direction[2] * t))
}

/// Whether the pointer may point at a cell: seen, and open floor or a door.
pub fn is_pointable(cell: Option<&SceneCell>) -> bool {
    cell.is_some_and(|c| c.visibility != Visibility::Unseen && (!c.occluder || c.kind == CellKind::Door))
}

/// Whether a billboard shows: on a cell seen before, and a monster only in sight.
pub fn is_shown(b: &Billboard, scene: &Scene) -> bool {
    let v = scene.cell(b.x, b.y).map_or(Visibility::Visible, |c| c.visibility);
    if b.kind == BillboardKind::Monster {
        v == Visibility::Visible
    } else {
        v != Visibility::Unseen
    }
}

/// Everything drawn, in world space; with `within`, only on the cells it takes, and with
/// `sprites_only`, only what stands.
pub fn plan_scene(
    scene: &Scene,
    yaw: f32,
    elevation: f32,
    view: &IsoView,
    focus: &[(i32, i32)],
    within: &dyn Fn(i32, i32) -> bool,
    sprites_only: bool,
) -> IsoPlan {
    let mut plan = IsoPlan::default();
    for cell in scene.cells.values() {
        if cell.visibility == Visibility::Unseen || !within(cell.x, cell.y) {
            continue;
        }
        let upright = cell.feature_tile.filter(|_| cell.stance == Some(isorun_scene::Stance::Upright));
        if sprites_only {
            if let (false, Some(t)) = (cell.occluder, upright) {
                plan.sprites.push(Sprite {
                    billboard: None,
                    x: cell.x,
                    y: cell.y,
                    layers: vec![SpriteLayer { tile: t, ox: 0.0, oy: 0.0, ymax: None }],
                    lying: false,
                    pile: false,
                });
            }
            continue;
        }
        if cell.occluder {
            let top = cell.wall_tile.unwrap_or(cell.floor_tile);
            let mut faces = Vec::new();
            for s in SIDE_NAMES {
                let n = side(s).normal;
                if scene.cell(cell.x + n.0, cell.y + n.1).is_some_and(|nb| nb.occluder && nb.visibility != Visibility::Unseen) {
                    continue;
                }
                let face = cell.face_tiles.and_then(|f| match s {
                    Side::N => f.n,
                    Side::S => f.s,
                    Side::W => f.w,
                    Side::E => f.e,
                });
                faces.push(WallFace { side: s, tile: face.unwrap_or(top) });
            }
            let height = wall_height_at(cell.x, cell.y, focus, yaw, elevation, view);
            plan.walls.push(WallBlock { x: cell.x, y: cell.y, height, top, faces });
            continue;
        }
        let mut layers = vec![cell.floor_tile];
        layers.extend(&cell.underlays);
        if let Some(t) = cell.feature_tile {
            if matches!(cell.stance, Some(isorun_scene::Stance::Lying) | Some(isorun_scene::Stance::Decal)) {
                layers.push(t);
            }
        }
        for id in &cell.overlays {
            if !cell.wall_shadows.contains(id) {
                layers.push(*id);
            }
        }
        for (lift, tile) in layers.into_iter().enumerate() {
            plan.ground.push(GroundLayer { x: cell.x, y: cell.y, tile, lift: lift as u32 });
        }
        if let Some(t) = upright {
            plan.sprites.push(Sprite {
                billboard: None,
                x: cell.x,
                y: cell.y,
                layers: vec![SpriteLayer { tile: t, ox: 0.0, oy: 0.0, ymax: None }],
                lying: false,
                pile: false,
            });
        }
    }
    for (i, b) in scene.billboards.iter().enumerate() {
        if !is_shown(b, scene) || !within(b.x, b.y) {
            continue;
        }
        let layers = if b.layers.is_empty() {
            vec![SpriteLayer { tile: b.tile, ox: 0.0, oy: 0.0, ymax: None }]
        } else {
            b.layers.iter().map(|l| SpriteLayer { tile: l.tile, ox: l.ox, oy: l.oy, ymax: l.ymax }).collect()
        };
        plan.sprites.push(Sprite { billboard: Some(i), x: b.x, y: b.y, layers, lying: b.lying, pile: b.kind == BillboardKind::Item && has_items_under(b) });
    }
    plan
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileFrame {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub texel_height: f32,
}

/// Where a tile's art is drawn within its cell, in cells, from the cell's top-left corner.
pub fn tile_frame(rect: &TileRect, ox: f32, oy: f32, ymax: Option<f32>) -> TileFrame {
    let h = match ymax {
        Some(m) => (m - rect.oy as f32).clamp(0.0, rect.h as f32),
        None => rect.h as f32,
    };
    let cell = rect.cell as f32;
    TileFrame { left: (rect.ox as f32 + ox) / cell, top: (rect.oy as f32 + oy) / cell, width: rect.w as f32 / cell, height: h / cell, texel_height: h }
}

const TORCH_EVERY: f64 = 5.0;

/// Whether a wall carries a torch on this face: about one face in five, the same every time.
pub fn has_torch(x: i32, y: i32, s: Side) -> bool {
    let idx = match s {
        Side::N => 0,
        Side::S => 1,
        Side::W => 2,
        Side::E => 3,
    };
    // the same 32-bit arithmetic as the JS (`| 0`, `Math.imul`, `>>>`)
    let h0 = (x as i64 * 374761393 + y as i64 * 668265263 + (idx + 7) as i64 * 2147483647) as i64;
    let mut h = to_int32(h0);
    h = (h ^ ((h as u32) >> 13) as i32).wrapping_mul(1274126177);
    let v = (h ^ ((h as u32) >> 16) as i32) as u32;
    (v as f64 / 4294967296.0) * TORCH_EVERY < 1.0
}

/// ECMAScript ToInt32 of a value exact in f64 (the products here stay below 2^53).
fn to_int32(v: i64) -> i32 {
    (v.rem_euclid(1 << 32)) as u32 as i32
}

pub const TORCH_HEIGHT: f32 = 0.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steering_follows_the_yaw() {
        assert_eq!(stick_to_dir(0.0, -1.0, 0.0), 0);
        assert_eq!(stick_to_dir(1.0, 0.0, 0.0), 2);
        assert_eq!(stick_to_dir(0.0, -1.0, std::f32::consts::FRAC_PI_2), 2);
    }

    #[test]
    fn about_one_face_in_five_has_a_torch() {
        let mut n = 0;
        for x in 0..40 {
            for y in 0..40 {
                n += has_torch(x, y, Side::N) as i32;
            }
        }
        assert!((250..390).contains(&n), "{n}");
    }
}
