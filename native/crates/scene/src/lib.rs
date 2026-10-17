//! The renderer-facing model of a DCSS level, with no knowledge of where it came from (the
//! port of `packages/scene`): a [`Scene`] of [`SceneCell`]s and [`Billboard`]s, a [`Camera`],
//! a [`Viewmodel`] for what is held, and the tile provider contract ([`TileRect`]).

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

pub mod bars;
pub mod motion;

pub use bars::*;
pub use motion::*;

pub type TileId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// 8 compass headings, clockwise from north. North is -y.
pub type Dir8 = u8;

pub const DIR8_DX: [i32; 8] = [0, 1, 1, 1, 0, -1, -1, -1];
pub const DIR8_DY: [i32; 8] = [-1, -1, 0, 1, 1, 1, 0, -1];

pub fn dir_from_delta(dx: i32, dy: i32) -> Option<Dir8> {
    let (sx, sy) = (dx.signum(), dy.signum());
    (0..8).find(|&i| DIR8_DX[i] == sx && DIR8_DY[i] == sy).map(|i| i as Dir8)
}

pub fn rotate_dir(d: Dir8, by: i32) -> Dir8 {
    (((d as i32 + by) % 8 + 8) % 8) as Dir8
}

pub fn dir_to_yaw(d: Dir8) -> f64 {
    d as f64 * PI / 4.0
}

pub fn yaw_to_dir(yaw: f64) -> Dir8 {
    let q = (yaw / (PI / 4.0)).round() as i64;
    ((q % 8 + 8) % 8) as Dir8
}

pub fn normalize_yaw(yaw: f64) -> f64 {
    let y = yaw % TAU;
    if y < 0.0 {
        y + TAU
    } else {
        y
    }
}

/// Signed shortest angular distance from a to b.
pub fn yaw_delta(a: f64, b: f64) -> f64 {
    let mut d = (b - a) % TAU;
    if d > PI {
        d -= TAU
    }
    if d < -PI {
        d += TAU
    }
    d
}

pub type CellKey = u32;

/// Coordinates in [-512, 512) pack into one integer key.
pub fn cell_key(x: i32, y: i32) -> CellKey {
    (((y + 512) << 10) | (x + 512)) as CellKey
}

pub fn key_to_xy(key: CellKey) -> (i32, i32) {
    ((key & 1023) as i32 - 512, (key >> 10) as i32 - 512)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum CellKind {
    #[default]
    Unknown,
    Floor,
    Wall,
    Door,
    Feature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Visibility {
    Visible,
    Remembered,
    #[default]
    Unseen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LiquidKind {
    Water,
    Murky,
    Starwater,
    Bog,
    Mud,
    Slime,
    Acid,
    Lava,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Liquid {
    pub kind: LiquidKind,
    pub deep: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UpDown {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DoorState {
    Open,
    Closed,
    Runed,
    Sealed,
}

/// What stands in a cell. `Stairs::branch` is the stair into a branch; `Stairs::exit` is
/// crawl's `DNGN_EXIT_DUNGEON`, where `<` ends the game.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Feature {
    Stairs { dir: UpDown, branch: bool, exit: bool },
    Hatch { dir: UpDown },
    Door { state: DoorState },
    Altar { god: Option<String> },
    Shop,
    Fountain { kind: String },
    Portal,
    Transporter,
    Trap { kind: Option<String> },
    Other { name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stance {
    Upright,
    Lying,
    Decal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WallStyle {
    Solid,
    Transparent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Beacon {
    Down,
    Up,
    Portal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CursorKind {
    Tutorial,
    Cursor1,
    Cursor2,
    Cursor3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct FaceTiles {
    pub n: Option<TileId>,
    pub s: Option<TileId>,
    pub w: Option<TileId>,
    pub e: Option<TileId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Trail {
    pub from: Option<Dir8>,
    pub to: Option<Dir8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct CellFlags {
    pub water: bool,
    pub lava: bool,
    pub excluded: bool,
    pub travel_trail: bool,
    pub new_stair: bool,
    pub cursor: bool,
    pub cursor_kind: Option<CursorKind>,
    /// Out of range for the current targeting prompt.
    pub out_of_range: bool,
    /// Known through magic mapping only, never seen.
    pub magic_mapped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// One cell of the level, classified. See `packages/scene/src/index.ts` for what each field
/// means to a renderer; the names follow it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SceneCell {
    pub x: i32,
    pub y: i32,
    pub kind: CellKind,
    pub visibility: Visibility,
    /// Stands full height: wall, closed door, void.
    pub occluder: bool,
    pub floor_tile: TileId,
    pub wall_tile: Option<TileId>,
    pub wall_style: Option<WallStyle>,
    pub face_tiles: Option<FaceTiles>,
    pub ceiling_tile: Option<TileId>,
    pub feature_tile: Option<TileId>,
    pub feature: Option<Feature>,
    pub stance: Option<Stance>,
    pub freestanding: bool,
    pub beacon: Option<Beacon>,
    pub underlays: Vec<TileId>,
    pub overlays: Vec<TileId>,
    pub wall_shadows: Vec<TileId>,
    pub shorelines: Vec<TileId>,
    pub liquid: Option<Liquid>,
    pub wall_overlays: Vec<TileId>,
    pub translucent: Vec<TileId>,
    pub icons: Vec<TileId>,
    pub cloud: Option<TileId>,
    pub trail: Option<Trail>,
    pub flags: CellFlags,
    pub flash: Option<Rgba8>,
    pub glyph: Option<String>,
    pub colour: Option<i32>,
    pub label: Option<String>,
    pub mf: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpriteEffect {
    Petrified,
    Burning,
    Poisoned,
    Corroded,
    Berserk,
    Glowing,
    Summoned,
    Wet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BillboardKind {
    Monster,
    Item,
    Cloud,
    Doll,
    Projectile,
    Player,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Attitude {
    Hostile,
    Neutral,
    GoodNeutral,
    Friendly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Threat {
    Trivial,
    Easy,
    Tough,
    Nasty,
    Invisible,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Layer {
    pub tile: TileId,
    pub ox: f32,
    pub oy: f32,
    pub ymax: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatusIcon {
    pub tile: TileId,
    pub ox: f32,
    pub oy: f32,
    /// Pins the badge's opaque texels to the top edge of the frame (the damage bar).
    pub at_top: bool,
    /// About the square, not the thing standing on it (items under here).
    pub square: bool,
    /// The whole cell: the net and the web.
    pub full: bool,
}

/// Something that stands in a cell: a monster, an item, a cloud, the player.
#[derive(Clone, Debug, PartialEq)]
pub struct Billboard {
    pub x: i32,
    pub y: i32,
    pub tile: TileId,
    pub kind: BillboardKind,
    /// Fraction of wall height, already resolved by the builder.
    pub height: f32,
    /// Doll / mcache composition, bottom to top.
    pub layers: Vec<Layer>,
    pub status_icons: Vec<StatusIcon>,
    pub effects: Vec<SpriteEffect>,
    /// Opacity 0..1 when the thing is translucent.
    pub alpha: Option<f32>,
    /// Lies flat on the floor: a corpse or a skeleton.
    pub lying: bool,
    pub attitude: Option<Attitude>,
    pub threat: Option<Threat>,
    pub name: Option<String>,
    pub scenery: bool,
    pub damage: Option<WoundLevel>,
    /// The monster's id (`mon.id`), which keeps an actor across scenes; the player is `PLAYER_REF`.
    pub ref_id: Option<i64>,
}

pub const PLAYER_REF: i64 = -1;

impl Billboard {
    pub fn new(x: i32, y: i32, tile: TileId, kind: BillboardKind, height: f32) -> Self {
        Billboard {
            x,
            y,
            tile,
            kind,
            height,
            layers: Vec::new(),
            status_icons: Vec::new(),
            effects: Vec::new(),
            alpha: None,
            lying: false,
            attitude: None,
            threat: None,
            name: None,
            scenery: false,
            damage: None,
            ref_id: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sky {
    None,
    Open,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneLevel {
    pub ceiling_tile: Option<TileId>,
    pub sky: Sky,
    /// Ambient light tint, 0..1 each.
    pub tint: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub bounds: Rect,
    pub cells: HashMap<CellKey, SceneCell>,
    pub player: (i32, i32),
    /// Whether the player is on this level at all (false while viewing a remote level).
    pub player_on_level: bool,
    pub billboards: Vec<Billboard>,
    pub level: SceneLevel,
    /// Increments whenever the scene is rebuilt.
    pub revision: u64,
    /// The revision at which the level's layout last changed.
    pub layout_revision: u64,
}

impl Default for Scene {
    fn default() -> Self {
        Scene {
            bounds: Rect { left: 0, top: 0, right: -1, bottom: -1 },
            cells: HashMap::new(),
            player: (0, 0),
            player_on_level: false,
            billboards: Vec::new(),
            level: SceneLevel { ceiling_tile: None, sky: Sky::None, tint: [1.0, 1.0, 1.0] },
            revision: 0,
            layout_revision: 0,
        }
    }
}

/// Whether two cells build the same level geometry. Neither visibility nor the flash is part
/// of it: both are colour a renderer lays over what is built.
pub fn cell_layout_equals(a: &SceneCell, b: &SceneCell) -> bool {
    a.kind == b.kind
        && a.occluder == b.occluder
        && a.floor_tile == b.floor_tile
        && a.wall_tile == b.wall_tile
        && a.ceiling_tile == b.ceiling_tile
        && a.feature_tile == b.feature_tile
        && a.stance == b.stance
        && a.wall_style == b.wall_style
        && a.underlays == b.underlays
        && a.overlays == b.overlays
        && a.icons == b.icons
        && a.wall_overlays == b.wall_overlays
        && a.wall_shadows == b.wall_shadows
        && a.shorelines == b.shorelines
        && a.translucent == b.translucent
        && a.face_tiles.unwrap_or_default() == b.face_tiles.unwrap_or_default()
        && a.trail.unwrap_or_default() == b.trail.unwrap_or_default()
}

/// Whether `next` has the same layout as `prev` (`cell_layout_equals` for every cell, the same
/// bounds, lid, sky and player-on-level).
pub fn scene_layout_equals(prev: &Scene, next: &Scene) -> bool {
    if prev.bounds != next.bounds {
        return false;
    }
    if prev.level.ceiling_tile != next.level.ceiling_tile || prev.level.sky != next.level.sky {
        return false;
    }
    if prev.player_on_level != next.player_on_level || prev.cells.len() != next.cells.len() {
        return false;
    }
    next.cells.iter().all(|(k, c)| prev.cells.get(k).is_some_and(|p| cell_layout_equals(p, c)))
}

pub const MEMORY_SHADE: f32 = 0.45;

/// How bright a cell is drawn: dimmed out of sight, falling off with the distance from the player.
pub fn shade_of(cell: Option<&SceneCell>, scene: &Scene) -> f32 {
    let Some(cell) = cell else { return 1.0 };
    let mut s = 1.0;
    if cell.visibility != Visibility::Visible {
        s *= MEMORY_SHADE;
    }
    let d = (((cell.x - scene.player.0) as f32).powi(2) + ((cell.y - scene.player.1) as f32).powi(2)).sqrt();
    s * (0.55 + 0.45 * (1.0 - d / 9.0).max(0.0))
}

/// The wash over a cell, each channel 0..1, 0 where the cell is not flashed.
pub fn flash_of(cell: Option<&SceneCell>) -> [f32; 4] {
    match cell.and_then(|c| c.flash) {
        Some(f) if f.a > 0 => [f.r as f32 / 255.0, f.g as f32 / 255.0, f.b as f32 / 255.0, (f.a as f32 / 255.0).min(1.0)],
        _ => [0.0; 4],
    }
}

impl Scene {
    pub fn cell(&self, x: i32, y: i32) -> Option<&SceneCell> {
        self.cells.get(&cell_key(x, y))
    }

    pub fn cell_under(&self) -> Option<&SceneCell> {
        self.cell(self.player.0, self.player.1)
    }

    pub fn cell_ahead(&self, facing: Dir8, from: Option<(i32, i32)>) -> Option<&SceneCell> {
        let p = from.unwrap_or(self.player);
        self.cell(p.0 + DIR8_DX[facing as usize], p.1 + DIR8_DY[facing as usize])
    }

    pub fn billboards_at(&self, x: i32, y: i32) -> impl Iterator<Item = &Billboard> {
        self.billboards.iter().filter(move |b| b.x == x && b.y == y)
    }

    /// Every monster on a visible cell, in the builder's order.
    pub fn monsters_in_view(&self) -> Vec<&Billboard> {
        self.billboards
            .iter()
            .filter(|b| b.kind == BillboardKind::Monster && self.cell(b.x, b.y).is_some_and(|c| c.visibility == Visibility::Visible))
            .collect()
    }

    /// The closest visible threat.
    pub fn nearest_hostile(&self) -> Option<&Billboard> {
        let c: Vec<_> = self.monsters_in_view().into_iter().filter(|b| is_threat(b)).collect();
        self.nearest_of(&c, |_| 0)
    }

    /// The closest visible monster that blocks explore; a hostile outranks a neutral.
    pub fn nearest_blocker(&self) -> Option<&Billboard> {
        let c: Vec<_> = self.monsters_in_view().into_iter().filter(|b| blocks_explore(b)).collect();
        self.nearest_of(&c, |b| if b.attitude == Some(Attitude::Neutral) { 1 } else { 0 })
    }

    /// The candidate closest to the player: fewest king moves, then `rank`, then distance.
    pub fn nearest_of<'a>(&self, candidates: &[&'a Billboard], rank: impl Fn(&Billboard) -> i64) -> Option<&'a Billboard> {
        let (px, py) = self.player;
        let mut best = None;
        let mut best_key = i64::MAX;
        for &b in candidates {
            let dx = (b.x - px) as i64;
            let dy = (b.y - py) as i64;
            let key = dx.abs().max(dy.abs()) * 10000 + rank(b) * 1000 + dx * dx + dy * dy;
            if key < best_key {
                best_key = key;
                best = Some(b);
            }
        }
        best
    }

    fn autofight_rank(&self, b: &Billboard) -> [i64; 4] {
        let dist = (b.x - self.player.0).abs().max((b.y - self.player.1).abs()) as i64;
        let injury = b.damage.map(|d| d as i64).unwrap_or(0);
        let threat = match b.threat {
            Some(Threat::Trivial) | None | Some(Threat::Invisible) => 0,
            Some(Threat::Easy) => 1,
            Some(Threat::Tough) => 2,
            Some(Threat::Nasty) => 3,
        };
        let orc = matches!(b.name.as_deref(), Some("orc priest") | Some("orc wizard")) as i64;
        [-dist, injury, threat, orc]
    }

    /// `compare_monster_info` order: > 0 when `a` ranks above `b`.
    pub fn compare_autofight(&self, a: &Billboard, b: &Billboard) -> i64 {
        let (ra, rb) = (self.autofight_rank(a), self.autofight_rank(b));
        (0..4).map(|i| ra[i] - rb[i]).find(|&d| d != 0).unwrap_or(0)
    }

    /// The threat autofight picks; a tie goes to the westernmost, then the northernmost.
    pub fn autofight_target(&self) -> Option<&Billboard> {
        let mut best: Option<&Billboard> = None;
        for b in self.monsters_in_view().into_iter().filter(|b| is_threat(b)) {
            match best {
                None => best = Some(b),
                Some(cur) => {
                    let c = self.compare_autofight(b, cur);
                    if c > 0 || (c == 0 && (b.x < cur.x || (b.x == cur.x && b.y < cur.y))) {
                        best = Some(b)
                    }
                }
            }
        }
        best
    }

    pub fn bearing_to(&self, x: i32, y: i32) -> Option<Dir8> {
        dir_from_delta(x - self.player.0, y - self.player.1)
    }
}

/// Whether more items lie on the billboard's square than it shows.
pub fn has_items_under(b: &Billboard) -> bool {
    b.status_icons.iter().any(|i| i.square)
}

/// A monster autofight would attack: hostile and not scenery.
pub fn is_threat(b: &Billboard) -> bool {
    b.kind == BillboardKind::Monster && b.attitude == Some(Attitude::Hostile) && !b.scenery
}

/// Whether a monster stops autoexplore and travel.
pub fn blocks_explore(b: &Billboard) -> bool {
    b.kind == BillboardKind::Monster && !b.scenery && matches!(b.attitude, Some(Attitude::Hostile) | Some(Attitude::Neutral))
}

/// Passable for walking purposes: known ground that is not an occluder.
pub fn is_walkable(cell: Option<&SceneCell>) -> bool {
    match cell {
        Some(c) => c.kind != CellKind::Unknown && !c.occluder && !c.flags.lava,
        None => false,
    }
}

/// One held thing drawn as its item icon layers in the 32-texel frame, bottom to top.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HandItem {
    pub layers: Vec<HandLayer>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct HandLayer {
    pub tile: TileId,
    /// The tile module the id belongs to when the source says so; unset resolves by range.
    pub layer: Option<String>,
    pub ymax: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Viewmodel {
    pub weapon: Option<HandItem>,
    pub offhand: Option<HandItem>,
}

/// Where a tile is in an atlas.
#[derive(Clone, Debug, PartialEq)]
pub struct TileRect {
    pub atlas: String,
    /// Source rect in texels.
    pub sx: u32,
    pub sy: u32,
    pub w: u32,
    pub h: u32,
    /// Offset of the drawn rect within a 32x32 cell, in texels.
    pub ox: i32,
    pub oy: i32,
    /// Nominal cell size the tile was authored for.
    pub cell: u32,
}

/// The camera: the cell it belongs to, where the eye stands now, the turn and the tilt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub x: i32,
    pub y: i32,
    pub eye_x: f64,
    pub eye_y: f64,
    /// Radians, continuous, 0 = north, clockwise.
    pub yaw: f64,
    pub pitch: f64,
    pub facing: Dir8,
}

pub const REST_PITCH: f64 = -(PI / 180.0) * 5.0;

impl Camera {
    pub fn new(x: i32, y: i32, facing: Dir8) -> Self {
        Camera { x, y, eye_x: x as f64, eye_y: y as f64, yaw: dir_to_yaw(facing), pitch: REST_PITCH, facing }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorMode {
    Target,
    Examine,
    Map,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneCursor {
    pub x: i32,
    pub y: i32,
    pub mode: CursorMode,
    pub tile: Option<TileId>,
    pub grid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for (x, y) in [(0, 0), (-5, 7), (79, 20), (-512, 511)] {
            assert_eq!(key_to_xy(cell_key(x, y)), (x, y));
        }
    }

    #[test]
    fn directions() {
        assert_eq!(dir_from_delta(0, -3), Some(0));
        assert_eq!(dir_from_delta(-1, -1), Some(7));
        assert_eq!(dir_from_delta(0, 0), None);
        assert_eq!(rotate_dir(7, 2), 1);
        assert_eq!(yaw_to_dir(dir_to_yaw(5)), 5);
        assert!((yaw_delta(0.1, TAU - 0.1) + 0.2).abs() < 1e-9);
    }
}
