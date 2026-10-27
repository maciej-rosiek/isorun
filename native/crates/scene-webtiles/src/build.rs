//! `GameState` + `Gamedata` → `Scene`, whole or only where the reducer touched it.

use std::collections::{HashMap, HashSet};

use isorun_gamedata::{FlagWord, Gamedata, Ranges};
use isorun_scene::{
    cell_key, cell_layout_equals, key_to_xy, scene_layout_equals, Billboard, BillboardKind, CellFlags, CellKey, CellKind, CursorKind, Layer, Liquid,
    LiquidKind, Rect, Rgba8, Scene, SceneCell, SpriteEffect, Stance, StatusIcon, Threat, TileId, Trail, Visibility, WallStyle, WoundLevel, PLAYER_REF,
};
use isorun_webtiles::{map_key, CellTiles, GameState, MapCell, Monster, TileWord};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

use crate::classify::{
    beacon_for, classify_feature_with, door_state_name, feature_label, gate_faces, is_rubble, is_scenery, is_vegetation, item_rarity, item_tile_name,
    level_presentation, liquid_label, liquid_of, stance_for, stands_free,
};
use crate::enums::{halo_table, HaloTable, MfTable};
use crate::monsters::{attitude_of, threat_of};

/// How far, in cells, a wall's masonry reaches out over the floor beside it. A room this
/// wide or narrower is roofed entirely in its own wall type.
pub const CEILING_REACH: i32 = 3;
/// How tall a standing item is, in cells: under the smallest monsters' heads, so one behind
/// it shows over it.
pub const ITEM_HEIGHT: f32 = 0.3;
/// How long a corpse or skeleton lies on the floor, in cells.
pub const LYING_LENGTH: f32 = 0.6;

const FLASH_COLOURS: [[u8; 4]; 16] = [
    [0, 0, 0, 0],
    [0, 0, 128, 100],
    [0, 128, 0, 100],
    [0, 128, 128, 100],
    [128, 0, 0, 100],
    [150, 0, 150, 100],
    [165, 91, 0, 100],
    [50, 50, 50, 150],
    [0, 0, 0, 150],
    [64, 64, 255, 100],
    [64, 255, 64, 100],
    [0, 255, 255, 100],
    [255, 64, 64, 100],
    [255, 64, 255, 100],
    [150, 150, 0, 100],
    [255, 255, 255, 100],
];

// ---------------------------------------------------------------------------
// Tile ids and flags by name, resolved once per build

macro_rules! names {
    ($ty:ident, $list:ident, $index:ident { $($n:ident),* $(,)? }) => {
        /// Every name of this table, for [`missing_tile_names`].
        const $list: &[&str] = &[$(stringify!($n)),*];

        #[allow(non_snake_case)]
        #[derive(Clone, Copy, Debug, Default)]
        struct $ty {
            $($n: Option<TileId>,)*
        }

        impl $ty {
            fn new(index: &isorun_gamedata::NameIndex) -> Self {
                $ty { $($n: index.id(stringify!($n)),)* }
            }
        }
    };
}

names!(
    DngnIds,
    DNGN_NAMES,
    dngn {
        DNGN_SHALLOW_WATER,
        DNGN_WALL_SHADOW,
        DNGN_WALL_SHADOW_DARK,
        LIQUEFACTION,
        BLOOD,
        MOLD,
        GLOWING_MOLD,
        WALL_BLOOD_S,
        RAY,
        RAY_MULTI,
        RAY_OUT_OF_RANGE,
        KRAKEN_OVERLAY_NW,
        KRAKEN_OVERLAY_NE,
        KRAKEN_OVERLAY_SE,
        KRAKEN_OVERLAY_SW,
        ELDRITCH_OVERLAY_NW,
        ELDRITCH_OVERLAY_NE,
        ELDRITCH_OVERLAY_SE,
        ELDRITCH_OVERLAY_SW,
        SANCTUARY,
        BLASPHEMY,
        BLOOD_FOR_BLOOD,
        SILENCED,
        HALO_RANGE,
        UMBRA,
        ORB_GLOW,
        QUAD_GLOW,
        DISJUNCT,
        HALO_FRIENDLY,
        HALO_GD_NEUTRAL,
        HALO_NEUTRAL,
        HALO_SUMMONER,
        THREAT_TRIVIAL,
        THREAT_EASY,
        THREAT_TOUGH,
        THREAT_NASTY,
        THREAT_UNUSUAL,
        THREAT_GHOST_TRIVIAL,
        THREAT_GHOST_EASY,
        THREAT_GHOST_TOUGH,
        THREAT_GHOST_NASTY,
        TRAVEL_EXCLUSION_BG,
        TRAVEL_EXCLUSION_CENTRE_BG,
        REMEMBERED_INVIS,
    }
);

names!(
    IconIds,
    ICON_NAMES,
    icons {
        BERSERK,
        TRAP_NET,
        TRAP_WEB,
        ITEM_STACK_1,
        ITEM_STACK_2,
        ITEM_STACK_3,
        SOMETHING_UNDER,
        FRIENDLY,
        GOOD_NEUTRAL,
        NEUTRAL,
        UNSEEN_INVIS_REMEMBERED,
        PARALYSED,
        STAB_BRAND,
        UNAWARE,
        FLEEING,
        POISON,
        MORE_POISON,
        MAX_POISON,
        RAMPAGE,
        NEW_STAIR,
        NEW_TRANSPORTER,
        TRAVEL_EXCLUSION_FG,
        TRAVEL_EXCLUSION_CENTRE_FG,
        TUTORIAL_CURSOR,
        CURSOR,
        CURSOR2,
        CURSOR3,
        TRAVEL_PATH_FROM,
        TRAVEL_PATH_TO,
        MDAM_LIGHTLY_DAMAGED,
        MDAM_MODERATELY_DAMAGED,
        MDAM_HEAVILY_DAMAGED,
        MDAM_SEVERELY_DAMAGED,
        MDAM_ALMOST_DEAD,
        DEMON_NUM1,
        DEMON_NUM2,
        DEMON_NUM3,
        DEMON_NUM4,
        DEMON_NUM5,
    }
);

names!(MainIds, MAIN_NAMES, main { PARCHMENT_LOW, PARCHMENT_HIGH });

/// Names a server may legitimately not publish: the "something under" badge is one icon up
/// to 0.34 (`SOMETHING_UNDER`) and three in master (`ITEM_STACK_1..3`), and the
/// remembered-invisible marks arrived in master with the flags that use them.
const OPTIONAL_NAMES: &[&str] = &["ITEM_STACK_1", "ITEM_STACK_2", "ITEM_STACK_3", "SOMETHING_UNDER", "UNSEEN_INVIS_REMEMBERED", "REMEMBERED_INVIS"];

/// Every tile the builder draws by name, checked against what this server's tileinfo modules
/// publish: empty on a server the client knows, a line per name otherwise. A name that does
/// not resolve means the tile was renamed and its decoration has silently stopped being drawn.
pub fn missing_tile_names(gd: &Gamedata) -> Vec<String> {
    let mut out = Vec::new();
    let optional = |n: &str| OPTIONAL_NAMES.contains(&n);
    for n in DNGN_NAMES {
        if !optional(n) && gd.dngn.id(n).is_none() {
            out.push(format!("tileinfo has no dungeon tile {n}"));
        }
    }
    for n in ICON_NAMES {
        if !optional(n) && gd.icons.id(n).is_none() {
            out.push(format!("tileinfo has no icon {n}"));
        }
    }
    for n in MAIN_NAMES {
        if !optional(n) && gd.main.id(n).is_none() {
            out.push(format!("tileinfo has no main tile {n}"));
        }
    }
    // the "something under" badge under either name: without one of them the flag draws nothing
    if gd.icons.id("ITEM_STACK_1").is_none() && gd.icons.id("SOMETHING_UNDER").is_none() {
        out.push("tileinfo has neither ITEM_STACK_1 nor SOMETHING_UNDER".into());
    }
    out
}

macro_rules! flags {
    ($ty:ident { $($n:ident),* $(,)? }) => {
        /// Where each flag the builder reads sits in the server's flag table.
        #[allow(non_snake_case)]
        #[derive(Clone, Copy, Debug, Default)]
        struct $ty {
            $($n: Option<usize>,)*
        }

        impl $ty {
            fn new(names: &[String]) -> Self {
                $ty { $($n: names.iter().position(|n| n == stringify!($n)),)* }
            }
        }
    };
}

flags!(BgFlags {
    WATER,
    TRAV_EXCL,
    EXCL_CTR,
    NEW_STAIR,
    NEW_TRANSPORTER,
    TUT_CURSOR,
    CURSOR1,
    CURSOR2,
    CURSOR3,
    OOR,
    MM_UNSEEN,
    UNSEEN,
    KRAKEN_NW,
    KRAKEN_NE,
    KRAKEN_SE,
    KRAKEN_SW,
    ELDRITCH_NW,
    ELDRITCH_NE,
    ELDRITCH_SE,
    ELDRITCH_SW,
    REMEMBERED_INVIS,
    RAMPAGE,
});

flags!(FgFlags {
    PET,
    GD_NEUTRAL,
    NEUTRAL,
    GHOST,
    TRIVIAL,
    EASY,
    TOUGH,
    NASTY,
    UNUSUAL,
    NET,
    WEB,
    S_UNDER,
    S_UNDER_GOOD,
    S_UNDER_ARTEFACT,
    PARALYSED,
    STAB,
    MAY_STAB,
    FLEEING,
    POISON,
    MORE_POISON,
    MAX_POISON,
    MDAM_LIGHT,
    MDAM_MOD,
    MDAM_HEAVY,
    MDAM_SEV,
    MDAM_ADEAD,
    DEMON_1,
    DEMON_2,
    DEMON_3,
    DEMON_4,
    DEMON_5,
});

fn on(w: &FlagWord, i: Option<usize>) -> bool {
    i.is_some_and(|i| w.is_set(i))
}

/// What one build reads of the gamedata, resolved once: the enum tables, the tiles drawn by
/// name, and the flags by their index.
struct Ctx<'g> {
    gd: &'g Gamedata,
    r: Ranges,
    mf: MfTable,
    halo: HaloTable,
    d: DngnIds,
    i: IconIds,
    m: MainIds,
    /// Wall shadow tiles (tilecell.cc `_pack_wall_shadows`), each with its variants.
    shadows: Vec<(i64, i64)>,
    /// Shoreline tiles (tilecell.cc `_pack_default_waves`, `_pack_shoal_waves`).
    shores: Vec<(i64, i64)>,
    bg: BgFlags,
    fg: FgFlags,
    demon_tier: bool,
}

impl<'g> Ctx<'g> {
    fn new(gd: &'g Gamedata, state: &GameState) -> Self {
        let d = DngnIds::new(&gd.dngn);
        let count = |id: i64| gd.tile_count(id as TileId) as i64;
        let mut shadows = Vec::new();
        for first in [d.DNGN_WALL_SHADOW, d.DNGN_WALL_SHADOW_DARK].into_iter().flatten() {
            let a = first as i64;
            shadows.push((a, a + count(a)));
        }
        // Shorelines come in runs of neighbouring tiles in dc-floor.txt: the five `SHORE_*`,
        // the shallow-water borders with their colour variations through the murky set, then
        // the deep, shallow and ink waves. A run is taken from its first named tile to the end
        // of its last, so the unnamed variations between them are covered too.
        let mut shores = Vec::new();
        for (first, last) in [("SHORE_N", "SHORE_NE"), ("DNGN_WAVE_N", "MURKY_WAVE_NW"), ("WAVE_DEEP_CORNER_NE", "WAVE_INK_FULL")] {
            let Some(a) = gd.dngn.id(first) else { continue };
            let a = a as i64;
            let z = gd.dngn.id(last).map_or(a, |z| z as i64);
            let end = a.max(z);
            shores.push((a, end + count(end)));
        }
        Ctx {
            gd,
            r: gd.ranges,
            mf: MfTable::for_gamedata(gd),
            halo: halo_table(gd),
            d,
            i: IconIds::new(&gd.icons),
            m: MainIds::new(&gd.main),
            shadows,
            shores,
            bg: BgFlags::new(gd.bg_flag_names()),
            fg: FgFlags::new(gd.fg_flag_names()),
            demon_tier: state.options.get("tile_show_demon_tier") == Some(&Value::Bool(true)),
        }
    }

    fn is_wall_shadow(&self, id: TileId) -> bool {
        self.shadows.iter().any(|&(a, b)| (id as i64) >= a && (id as i64) < b)
    }

    fn is_shoreline(&self, id: TileId) -> bool {
        self.shores.iter().any(|&(a, b)| (id as i64) >= a && (id as i64) < b)
    }

    fn bg(&self, w: Option<TileWord>) -> FlagWord {
        let w = w.unwrap_or_default();
        self.gd.bg_bits(w.lo, w.hi)
    }

    fn fg(&self, w: Option<TileWord>) -> FlagWord {
        let w = w.unwrap_or_default();
        self.gd.fg_bits(w.lo, w.hi)
    }
}

// ---------------------------------------------------------------------------
// The build

/// What the reducer touched since the previous scene was built: its `dirty_cells` and whether
/// the map was cleared.
#[derive(Clone, Copy, Debug)]
pub struct Dirty<'a> {
    pub cells: &'a HashSet<u32>,
    pub map_cleared: bool,
}

impl<'a> Dirty<'a> {
    /// The state's own `dirty_cells` and `map_cleared`; call `GameState::clear_dirty` once the
    /// scene is built.
    pub fn of(state: &'a GameState) -> Self {
        Dirty { cells: &state.dirty_cells, map_cleared: state.map_cleared }
    }
}

/// What a built scene was built from, kept beside it so the next build can tell which of it
/// still holds (the TS client keeps this in a `WeakMap` on the scene). Start with
/// `BuildRecord::default()` and hand the same record to every build of one scene chain.
#[derive(Clone, Debug, Default)]
pub struct BuildRecord {
    built: Option<Record>,
    carried: usize,
    rebuilt: usize,
}

impl BuildRecord {
    /// How many cells the last build carried over from the previous scene unchanged.
    pub fn carried(&self) -> usize {
        self.carried
    }

    /// How many cells the last build built afresh.
    pub fn rebuilt(&self) -> usize {
        self.rebuilt
    }
}

#[derive(Clone, Debug)]
struct Record {
    /// The scene this record describes, by its revision and size.
    revision: u64,
    cells: usize,
    /// The state and gamedata it was built from, by address (the TS compares the objects).
    state: usize,
    gd: usize,
    gd_fingerprint: String,
    player_on_level: bool,
    place: String,
    demon_tier: Option<Value>,
    /// The wall base tiles the scene's lids were voted from.
    walls: HashMap<CellKey, TileId>,
}

#[derive(Default)]
pub struct BuildOptions<'a> {
    /// Previous scene, reused for stability where possible.
    pub previous: Option<&'a Scene>,
    /// What the reducer touched since `previous` was built. With it (and a `record` that
    /// describes `previous`), every cell and billboard the messages left alone is carried over
    /// from `previous` as it is, and only the touched cells (and whatever depends on them: the
    /// lids near a changed wall, the cell the player left and the one they stand on) are built
    /// again. Without it, or after a clear, the whole level is built.
    pub dirty: Option<Dirty<'a>>,
    /// The record of the build of `previous`; updated to describe the new scene.
    pub record: Option<&'a mut BuildRecord>,
}

fn addr<T>(t: &T) -> usize {
    t as *const T as usize
}

/// Build the renderable scene from the game state. Pure: reads state and gamedata, never
/// mutates them. With `previous`, `dirty` and the `record`, cells the reducer did not touch
/// are copied from the previous scene as they are.
pub fn build_scene(state: &GameState, gd: &Gamedata, opts: BuildOptions) -> Scene {
    let BuildOptions { previous: prev, dirty, mut record } = opts;
    let ctx = Ctx::new(gd, state);
    let mut scene = Scene { revision: prev.map_or(0, |p| p.revision) + 1, ..Scene::default() };
    let map = &state.map;
    scene.player = (state.player.pos.x, state.player.pos.y);
    scene.player_on_level = map.player_on_level;
    scene.bounds = map.bounds.map_or(Rect { left: 0, top: 0, right: -1, bottom: -1 }, |b| Rect { left: b.left, top: b.top, right: b.right, bottom: b.bottom });
    let pres = level_presentation(&state.player.place);
    scene.level.sky = pres.sky;
    scene.level.tint = pres.tint;
    let place = state.player.place.clone();
    let demon_tier = state.options.get("tile_show_demon_tier").cloned();

    // the record of the previous scene, when it is the one handed in
    let was: Option<Record> =
        record.as_mut().and_then(|r| r.built.take()).filter(|w| prev.is_some_and(|p| p.revision == w.revision && p.cells.len() == w.cells));
    // the previous build stands for this state and gamedata, nothing global changed, and the reducer says what moved
    let incremental = match (prev, &was, dirty) {
        (Some(_), Some(w), Some(d)) => {
            !d.map_cleared
                && w.state == addr(state)
                && w.gd == addr(gd)
                && w.gd_fingerprint == gd.fingerprint
                && w.player_on_level == scene.player_on_level
                && w.place == place
                && w.demon_tier == demon_tier
        }
        _ => false,
    };
    // the cells to build afresh: the reducer's, and the player's old and new cells
    let rebuild: Option<HashSet<u32>> = incremental.then(|| {
        let mut set = dirty.map(|d| d.cells.clone()).unwrap_or_default();
        let p = prev.unwrap();
        if p.player != scene.player {
            set.insert(map_key(p.player.0, p.player.1));
            set.insert(map_key(scene.player.0, scene.player.1));
        }
        set
    });
    let mut wall_base: HashMap<CellKey, TileId> = match (&rebuild, &was) {
        (Some(_), Some(w)) => w.walls.clone(),
        _ => HashMap::new(),
    };
    // cells whose wall base changed this build: the lids within CEILING_REACH of them are voted on again
    let mut walls_changed: Vec<CellKey> = Vec::new();
    // whether anything a level mesh is built from changed: the cells built afresh compared with what stood
    let mut layout_changed = prev.is_none() || !incremental;
    // the billboards of each carried cell, as the previous scene has them
    let mut carried_billboards: HashMap<CellKey, Vec<&Billboard>> = HashMap::new();
    if rebuild.is_some() {
        for b in &prev.unwrap().billboards {
            carried_billboards.entry(cell_key(b.x, b.y)).or_default().push(b);
        }
    }
    let (mut carried, mut rebuilt) = (0, 0);
    // in key order (row by row), so a build does not depend on the hash map's order
    let mut keys: Vec<u32> = map.cells.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let mc = &map.cells[&k];
        let key = cell_key(mc.x, mc.y);
        let had = match (&rebuild, prev) {
            (Some(_), Some(p)) => p.cells.get(&key),
            _ => None,
        };
        if let (Some(had), Some(rb)) = (had, &rebuild) {
            if !rb.contains(&map_key(mc.x, mc.y)) {
                // untouched since the last build: the same cell and billboards stand
                scene.cells.insert(key, had.clone());
                if let Some(bbs) = carried_billboards.get(&key) {
                    scene.billboards.extend(bbs.iter().map(|b| (*b).clone()));
                }
                carried += 1;
                continue;
            }
        }
        rebuilt += 1;
        let mut cell = build_cell(mc, &ctx);
        let base = wall_of(gd, &cell);
        if rebuild.is_some() {
            if wall_base.get(&key).copied() != base {
                walls_changed.push(key);
                match base {
                    None => wall_base.remove(&key),
                    Some(b) => wall_base.insert(key, b),
                };
            }
        } else if let Some(b) = base {
            wall_base.insert(key, b);
        }
        add_billboards(&mut scene.billboards, mc, &mut cell, state, &ctx, scene.player_on_level);
        if rebuild.is_some() && !layout_changed && !had.is_some_and(|h| cell_layout_equals(h, &cell)) {
            layout_changed = true;
        }
        scene.cells.insert(key, cell);
    }
    if let (Some(_), Some(p)) = (&rebuild, prev) {
        for key in p.cells.keys() {
            if !scene.cells.contains_key(key) {
                // a cell the map no longer knows is a change of layout too, and its wall is gone with it
                layout_changed = true;
                if wall_base.remove(key).is_some() {
                    walls_changed.push(*key);
                }
            }
        }
    }
    // the dominant wall tile as the ceiling; a tie goes to the lower tile id
    let mut wall_counts: HashMap<TileId, usize> = HashMap::new();
    for &b in wall_base.values() {
        *wall_counts.entry(b).or_default() += 1;
    }
    scene.level.ceiling_tile = wall_counts.iter().map(|(&t, &n)| (n, std::cmp::Reverse(t))).max().map(|(_, t)| t.0);
    match (&rebuild, prev) {
        (Some(_), Some(p)) => {
            // the lids depend on the walls alone: cells carried over keep theirs unless a wall within reach changed,
            // and a new dominant wall type is a new lid for every cell out of reach of any wall
            if p.level.ceiling_tile != scene.level.ceiling_tile {
                assign_ceilings(&mut scene, &wall_base, None, None);
            } else {
                assign_ceilings(&mut scene, &wall_base, Some(p), Some(&near(&walls_changed, CEILING_REACH)));
            }
        }
        _ => {
            let reuse = match (prev, &was) {
                (Some(p), Some(w)) if p.level.ceiling_tile == scene.level.ceiling_tile && w.walls == wall_base => Some(p),
                _ => None,
            };
            assign_ceilings(&mut scene, &wall_base, reuse, None);
        }
    }
    sort_monsters(&mut scene);
    // the level's geometry stands across builds that only moved monsters, light or cursors
    match (&rebuild, prev) {
        (Some(_), Some(p)) => {
            if p.bounds != scene.bounds
                || p.level.ceiling_tile != scene.level.ceiling_tile
                || p.level.sky != scene.level.sky
                // a step is not a layout change: the light, the lowered walls and the feature underfoot follow the player
                || p.player_on_level != scene.player_on_level
                || p.cells.len() != scene.cells.len()
            {
                layout_changed = true;
            }
            scene.layout_revision = if layout_changed { scene.revision } else { p.layout_revision };
        }
        _ => {
            scene.layout_revision = match prev {
                Some(p) if scene_layout_equals(p, &scene) => p.layout_revision,
                _ => scene.revision,
            };
        }
    }
    if let Some(r) = record.as_mut() {
        r.carried = carried;
        r.rebuilt = rebuilt;
        r.built = Some(Record {
            revision: scene.revision,
            cells: scene.cells.len(),
            state: addr(state),
            gd: addr(gd),
            gd_fingerprint: gd.fingerprint.clone(),
            player_on_level: scene.player_on_level,
            place,
            demon_tier,
            walls: wall_base,
        });
    }
    scene
}

fn wall_of(gd: &Gamedata, cell: &SceneCell) -> Option<TileId> {
    if cell.kind == CellKind::Wall {
        cell.wall_tile.map(|t| gd.basetile(t))
    } else {
        None
    }
}

/// Keep monster ordering, threat then distance from the player, among the slots the monsters
/// hold; every other billboard keeps its place.
fn sort_monsters(scene: &mut Scene) {
    let (px, py) = scene.player;
    let slots: Vec<usize> = (0..scene.billboards.len()).filter(|&i| scene.billboards[i].kind == BillboardKind::Monster).collect();
    let mut mons: Vec<Billboard> = slots.iter().map(|&i| scene.billboards[i].clone()).collect();
    let dist = |b: &Billboard| (b.x - px).abs().max((b.y - py).abs());
    mons.sort_by(|a, b| threat_rank(b.threat).cmp(&threat_rank(a.threat)).then(dist(a).cmp(&dist(b))));
    for (slot, m) in slots.into_iter().zip(mons) {
        scene.billboards[slot] = m;
    }
}

fn threat_rank(t: Option<Threat>) -> i32 {
    match t {
        Some(Threat::Nasty) => 4,
        Some(Threat::Tough) | Some(Threat::Invisible) => 3,
        Some(Threat::Easy) => 2,
        Some(Threat::Trivial) => 1,
        None => 0,
    }
}

/// The cells within `reach` (Chebyshev) of any of `keys`.
fn near(keys: &[CellKey], reach: i32) -> HashSet<CellKey> {
    let mut out = HashSet::new();
    for &k in keys {
        let (x, y) = key_to_xy(k);
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                out.insert(cell_key(x + dx, y + dy));
            }
        }
    }
    out
}

/// Roof each open cell in the wall type nearest it. Every wall within `CEILING_REACH`
/// (Chebyshev) votes for its base tile, nearer walls counting more, so a vault of other
/// masonry keeps its own lid up to its doorway and a stray odd block does not recolour a whole
/// corridor. Cells out of reach of any wall, and ties, keep the level ceiling and get no
/// per-cell tile.
///
/// `reuse` is a scene voted from the same walls under the same level lid: a cell it has keeps
/// the lid it had there, and only cells new since then (or in `revote`, a wall within reach
/// having changed) are voted on. The votes are most of a build's cost and the walls seldom
/// change.
fn assign_ceilings(scene: &mut Scene, wall_base: &HashMap<CellKey, TileId>, reuse: Option<&Scene>, revote: Option<&HashSet<CellKey>>) {
    let Some(level) = scene.level.ceiling_tile else { return };
    let r = CEILING_REACH;
    for (&key, cell) in scene.cells.iter_mut() {
        if cell.kind == CellKind::Unknown || cell.occluder {
            continue;
        }
        if let Some(reuse) = reuse {
            if !revote.is_some_and(|v| v.contains(&key)) {
                if let Some(had) = reuse.cells.get(&key) {
                    if !had.occluder && had.kind != CellKind::Unknown {
                        if had.ceiling_tile.is_some() {
                            cell.ceiling_tile = had.ceiling_tile;
                        }
                        continue;
                    }
                }
            }
        }
        // the votes in the order cast, so a tie goes to the first as the TS's Map has it
        let mut votes: Vec<(TileId, i32)> = Vec::new();
        for dy in -r..=r {
            for dx in -r..=r {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let Some(&base) = wall_base.get(&cell_key(cell.x + dx, cell.y + dy)) else { continue };
                let w = r + 1 - dx.abs().max(dy.abs());
                match votes.iter_mut().find(|v| v.0 == base) {
                    Some(v) => v.1 += w,
                    None => votes.push((base, w)),
                }
            }
        }
        let mut pick = level;
        let mut pick_n = votes.iter().find(|v| v.0 == level).map_or(0, |v| v.1);
        for &(t, n) in &votes {
            if n > pick_n {
                pick = t;
                pick_n = n;
            }
        }
        cell.ceiling_tile = (!votes.is_empty() && pick != level).then_some(pick);
    }
}

// ---------------------------------------------------------------------------
// Cells

fn liquid_of_map(flags: &CellFlags, mf: Option<i64>, t: &MfTable) -> Option<Liquid> {
    if flags.lava {
        Some(Liquid { kind: LiquidKind::Lava, deep: true })
    } else if flags.water {
        Some(Liquid { kind: LiquidKind::Water, deep: mf != Some(t.water) })
    } else {
        None
    }
}

/// Tutorial cursor first, then CURSOR1..3, as cell_renderer.js draws them.
fn cursor_kind(bgw: &FlagWord, f: &BgFlags) -> Option<CursorKind> {
    if on(bgw, f.TUT_CURSOR) {
        Some(CursorKind::Tutorial)
    } else if on(bgw, f.CURSOR1) {
        Some(CursorKind::Cursor1)
    } else if on(bgw, f.CURSOR2) {
        Some(CursorKind::Cursor2)
    } else if on(bgw, f.CURSOR3) {
        Some(CursorKind::Cursor3)
    } else {
        None
    }
}

fn wall_label(name: Option<&str>) -> String {
    let n = name.filter(|n| !n.is_empty()).unwrap_or("wall");
    let n = n.strip_prefix("DNGN_").unwrap_or(n);
    let n = n.strip_prefix("WALL_").unwrap_or(n);
    n.to_lowercase().replace('_', " ")
}

static SEE_THROUGH: Lazy<Regex> = Lazy::new(|| Regex::new("TRANSPARENT|CRYSTAL|GLASS").unwrap());

fn build_cell(mc: &MapCell, ctx: &Ctx) -> SceneCell {
    let gd = ctx.gd;
    let (r, t_mf, bf) = (&ctx.r, &ctx.mf, &ctx.bg);
    let t = mc.t.as_ref();
    let bgw = ctx.bg(t.and_then(|t| t.bg));
    let bg_idx = bgw.value;
    let cloud = gd.server_tile(t.and_then(|t| t.cloud).map_or(0, |w| w.lo) & 0xffff, "main");
    let is = |v: i64| mc.mf == Some(v);
    let mut flags = CellFlags {
        water: on(&bgw, bf.WATER) || is(t_mf.water) || is(t_mf.deep_water),
        lava: is(t_mf.lava),
        excluded: on(&bgw, bf.TRAV_EXCL) || on(&bgw, bf.EXCL_CTR) || is(t_mf.excl) || is(t_mf.excl_root),
        travel_trail: t.and_then(|t| t.travel_trail).is_some_and(|tt| tt & 0xff != 0),
        new_stair: on(&bgw, bf.NEW_STAIR),
        cursor: [bf.TUT_CURSOR, bf.CURSOR1, bf.CURSOR2, bf.CURSOR3].iter().any(|&i| on(&bgw, i)),
        cursor_kind: cursor_kind(&bgw, bf),
        out_of_range: on(&bgw, bf.OOR),
        magic_mapped: on(&bgw, bf.MM_UNSEEN),
    };
    let glyph = mc.g.as_deref();
    let mut base = SceneCell {
        x: mc.x,
        y: mc.y,
        kind: CellKind::Unknown,
        visibility: Visibility::Unseen,
        occluder: true,
        floor_tile: 0,
        glyph: mc.g.clone(),
        colour: mc.col.map(|c| c as i32),
        mf: mc.mf.map(|m| m as i32),
        ..SceneCell::default()
    };
    if let Some(flc) = mc.flc.filter(|&f| f != 0) {
        let c = usize::try_from(flc).ok().and_then(|i| FLASH_COLOURS.get(i)).unwrap_or(&FLASH_COLOURS[0]);
        let a = mc.fla.filter(|&a| a != 0).map_or(c[3], |a| a.clamp(0, 255) as u8);
        base.flash = Some(Rgba8 { r: c[0], g: c[1], b: c[2], a });
    }
    let Some(t) = t.filter(|_| bg_idx != 0) else {
        // no terrain knowledge: keep it as unknown (void); a magic-mapped cell with mf but no tile too
        if mc.mf.is_some_and(|m| m != 0 && m != t_mf.unseen) && bg_idx == 0 {
            base.label = Some("unexplored".into());
        }
        base.flags = flags;
        return base;
    };
    base.visibility = if !on(&bgw, bf.UNSEEN) && !on(&bgw, bf.MM_UNSEEN) { Visibility::Visible } else { Visibility::Remembered };
    let name = gd.dngn.base_name(bg_idx).filter(|n| !n.is_empty()).or_else(|| gd.dngn.name(bg_idx));
    let liquid = liquid_of(gd.dngn.name(bg_idx))
        .or_else(|| liquid_of_map(&flags, mc.mf, t_mf))
        .or_else(|| t.liquefied.unwrap_or(false).then_some(Liquid { kind: LiquidKind::Mud, deep: false }));
    base.liquid = liquid;
    flags.water = liquid.is_some_and(|l| matches!(l.kind, LiquidKind::Water | LiquidKind::Murky | LiquidKind::Starwater));
    flags.lava = liquid.is_some_and(|l| l.kind == LiquidKind::Lava);
    base.flags = flags;
    let floor_under = t.flv.as_ref().and_then(|f| f.f).map_or(bg_idx, |f| f as TileId);
    let has_cloud = cloud > 0;
    let bg = bg_idx as i64;

    if bg < r.floor_max {
        base.kind = CellKind::Floor;
        base.occluder = false;
        base.floor_tile = bg_idx;
        base.label = Some(liquid.map_or("floor", |l| liquid_label(&l)).into());
        if glyph == Some("#") && !has_cloud {
            // glyph backstop: a vault wall using a floor-range tile
            base.kind = CellKind::Wall;
            base.occluder = true;
            base.wall_tile = Some(bg_idx);
            base.wall_style = Some(WallStyle::Solid);
            base.label = Some("wall".into());
        }
    } else if bg < r.first_transparent {
        base.kind = CellKind::Wall;
        base.occluder = true;
        base.wall_tile = Some(bg_idx);
        base.wall_style = Some(WallStyle::Solid);
        base.floor_tile = floor_under;
        base.label = Some(wall_label(name));
    } else if bg < r.wall_max {
        base.floor_tile = floor_under;
        if glyph == Some("#") && !has_cloud {
            base.kind = CellKind::Wall;
            base.occluder = true;
            base.wall_tile = Some(bg_idx);
            // glass and crystal walls are see-through; other art in this range is drawn over floor for art reasons
            base.wall_style = Some(if SEE_THROUGH.is_match(name.unwrap_or("")) { WallStyle::Transparent } else { WallStyle::Solid });
            base.label = Some(wall_label(name));
        } else {
            // transparent-range art that is not a wall: a decal or standing thing over floor
            base.kind = CellKind::Feature;
            base.occluder = false;
            base.feature_tile = Some(bg_idx);
            let f = classify_feature_with(name, glyph, mc.mf, t_mf);
            base.stance = Some(stance_for(name, f.as_ref()));
            base.freestanding = stands_free(name, f.as_ref());
            base.label = Some(feature_label(f.as_ref(), name));
            base.feature = f;
        }
    } else {
        // feature range
        base.floor_tile = floor_under;
        let f = classify_feature_with(name, glyph, mc.mf, t_mf);
        if glyph == Some("#") && !has_cloud && f.is_none() {
            base.kind = CellKind::Wall;
            base.occluder = true;
            base.wall_tile = Some(bg_idx);
            base.wall_style = Some(WallStyle::Solid);
            base.label = Some("wall".into());
        } else if let Some(isorun_scene::Feature::Door { state }) =
            f.as_ref().filter(|f| !matches!(f, isorun_scene::Feature::Door { state: isorun_scene::DoorState::Open }))
        {
            base.kind = CellKind::Door;
            base.occluder = true;
            base.feature_tile = Some(bg_idx);
            base.wall_tile = Some(bg_idx);
            base.face_tiles = gate_faces(gd, name);
            base.stance = Some(Stance::Upright);
            base.label = Some(format!("{} door", door_state_name(*state)));
            base.feature = f;
        } else {
            base.kind = CellKind::Feature;
            base.occluder = false;
            base.feature_tile = Some(bg_idx);
            base.stance = Some(stance_for(name, f.as_ref()));
            base.freestanding = stands_free(name, f.as_ref());
            base.beacon = beacon_for(f.as_ref());
            base.label = Some(feature_label(f.as_ref(), name));
            base.feature = f;
            // trees and statues block movement but not knowledge; passable for rendering
        }
    }
    decorate(&mut base, t, &bgw, ctx);
    if has_cloud {
        base.cloud = Some(cloud);
    }
    base
}

// ---------------------------------------------------------------------------
// Decorations. This follows the official client's draw_background and the icon pass of
// draw_foreground (cell_renderer.js) so that everything WebTiles paints on a cell has a place
// on the Scene: blood and mould, wall shadows and other server overlays, halos, sanctuary,
// silence, threat rings, travel exclusions, the travel trail, and the badges.

/// Variation of an animated or varied tile chosen by the cell's special flavour, as the
/// official client does.
fn variant(gd: &Gamedata, base: Option<TileId>, seed: i64) -> Option<TileId> {
    let base = base?;
    Some((base as i64 + seed % gd.tile_count(base).max(1) as i64) as TileId)
}

fn blood_tile(ctx: &Ctx, t: &CellTiles, on_wall: bool) -> Option<TileId> {
    let (gd, d) = (ctx.gd, &ctx.d);
    let seed = t.flv.as_ref().and_then(|f| f.s).unwrap_or(0);
    let yes = |b: Option<bool>| b.unwrap_or(false);
    if yes(t.liquefied) && !on_wall {
        return variant(gd, d.LIQUEFACTION, seed);
    }
    if yes(t.bloody) {
        if !on_wall {
            return variant(gd, d.BLOOD, seed);
        }
        let b = d.WALL_BLOOD_S?;
        let rotated = b as i64 + gd.tile_count(b) as i64 * t.blood_rotation.unwrap_or(0);
        return variant(gd, Some(rotated as TileId), seed);
    }
    if yes(t.moldy) {
        return variant(gd, d.MOLD, seed);
    }
    if yes(t.glowing_mold) {
        return variant(gd, d.GLOWING_MOLD, seed);
    }
    None
}

#[derive(Default)]
struct Decor {
    on_wall: bool,
    under: Vec<TileId>,
    over: Vec<TileId>,
    wall: Vec<TileId>,
    icons: Vec<TileId>,
    shadows: Vec<TileId>,
    shores: Vec<TileId>,
    translucent: Vec<TileId>,
}

impl Decor {
    /// A decal on the bg tile itself: on the wall face for walls, over the floor otherwise.
    fn push(&mut self, ctx: &Ctx, id: Option<TileId>) {
        let Some(id) = id.filter(|&i| i > 0) else { return };
        if self.on_wall {
            self.wall.push(id);
        } else {
            self.over.push(id);
            if ctx.is_wall_shadow(id) {
                self.shadows.push(id);
            }
            if ctx.is_shoreline(id) {
                self.shores.push(id);
            }
        }
        if Some(id) == ctx.d.TRAVEL_EXCLUSION_BG || Some(id) == ctx.d.TRAVEL_EXCLUSION_CENTRE_BG {
            self.translucent.push(id);
        }
    }
}

fn plus(base: Option<TileId>, by: i64) -> Option<TileId> {
    base.map(|b| (b as i64 + by) as TileId)
}

fn decorate(cell: &mut SceneCell, t: &CellTiles, bgw: &FlagWord, ctx: &Ctx) {
    let (r, d, i, bf, ff) = (&ctx.r, &ctx.d, &ctx.i, &ctx.bg, &ctx.fg);
    let bg = bgw.value as i64;
    let mut dc = Decor { on_wall: matches!(cell.kind, CellKind::Wall | CellKind::Door), ..Decor::default() };
    let ov: &[i64] = t.ov.as_deref().unwrap_or(&[]);
    let yes = |b: Option<bool>| b.unwrap_or(false);

    if yes(t.mangrove_water) && bg > 0 {
        // a mangrove stands in shallow water
        if let Some(w) = d.DNGN_SHALLOW_WATER {
            cell.floor_tile = w;
        }
    } else if bg >= r.first_transparent {
        // floor patterns beneath a transparent feature
        dc.under.extend(ov.iter().filter(|&&o| o > 0 && o < r.floor_max).map(|&o| o as TileId));
    }
    // blood beneath a feature tile
    if bg >= r.wall_max {
        if let Some(b) = blood_tile(ctx, t, false) {
            dc.under.push(b);
        }
    }
    if bg > 0 {
        // blood on top of floor and wall tiles
        if bg < r.wall_max {
            dc.push(ctx, blood_tile(ctx, t, bg >= r.floor_max));
        }
        // server overlays: beams last, main-range ones (zaps) are billboards
        let mut ray = None;
        for &o in ov {
            if o <= 0 || o >= r.feat_max {
                continue;
            }
            let id = o as TileId;
            if [d.RAY, d.RAY_MULTI, d.RAY_OUT_OF_RANGE].contains(&Some(id)) {
                ray = Some(id);
            } else if bg < r.first_transparent || o >= r.floor_max {
                dc.push(ctx, Some(id));
            }
        }
        if ray.is_some() {
            dc.push(ctx, ray);
        }
        if !on(bgw, bf.UNSEEN) {
            for (k, e, kt, et) in [
                (bf.KRAKEN_NW, bf.ELDRITCH_NW, d.KRAKEN_OVERLAY_NW, d.ELDRITCH_OVERLAY_NW),
                (bf.KRAKEN_NE, bf.ELDRITCH_NE, d.KRAKEN_OVERLAY_NE, d.ELDRITCH_OVERLAY_NE),
                (bf.KRAKEN_SE, bf.ELDRITCH_SE, d.KRAKEN_OVERLAY_SE, d.ELDRITCH_OVERLAY_SE),
                (bf.KRAKEN_SW, bf.ELDRITCH_SW, d.KRAKEN_OVERLAY_SW, d.ELDRITCH_OVERLAY_SW),
            ] {
                if on(bgw, k) {
                    dc.push(ctx, kt);
                } else if on(bgw, e) {
                    dc.push(ctx, et);
                }
            }
            if yes(t.sanctuary) {
                dc.push(ctx, d.SANCTUARY);
            }
            if yes(t.blasphemy) {
                dc.push(ctx, d.BLASPHEMY);
            }
            if yes(t.has_bfb_corpse) {
                dc.push(ctx, d.BLOOD_FOR_BLOOD);
            }
            if yes(t.silenced) {
                dc.push(ctx, d.SILENCED);
            }
            let h = ctx.halo;
            let halo = t.halo.unwrap_or(0);
            if halo == h.range {
                dc.push(ctx, d.HALO_RANGE);
            }
            if halo >= h.umbra_first && halo <= h.umbra_last {
                dc.push(ctx, plus(d.UMBRA, halo - h.umbra_first));
            }
            if let Some(g) = t.orb_glow.filter(|&g| g != 0) {
                dc.push(ctx, plus(d.ORB_GLOW, g - 1));
            }
            if yes(t.quad_glow) {
                dc.push(ctx, d.QUAD_GLOW);
            }
            if let Some(g) = t.disjunct.filter(|&g| g != 0) {
                dc.push(ctx, plus(d.DISJUNCT, g - 1));
            }
            if yes(t.awakened_forest) {
                dc.push(ctx, i.BERSERK);
            }

            let fgw = ctx.fg(t.fg);
            if fgw.value != 0 || t.fg.is_some_and(|w| w.value() != 0) {
                if on(&fgw, ff.PET) {
                    dc.push(ctx, d.HALO_FRIENDLY);
                } else if on(&fgw, ff.GD_NEUTRAL) {
                    dc.push(ctx, d.HALO_GD_NEUTRAL);
                } else if on(&fgw, ff.NEUTRAL) {
                    dc.push(ctx, d.HALO_NEUTRAL);
                }
                // monster threat; ghosts get their own ring
                let rings = if on(&fgw, ff.GHOST) {
                    [d.THREAT_GHOST_TRIVIAL, d.THREAT_GHOST_EASY, d.THREAT_GHOST_TOUGH, d.THREAT_GHOST_NASTY, d.THREAT_UNUSUAL]
                } else {
                    [d.THREAT_TRIVIAL, d.THREAT_EASY, d.THREAT_TOUGH, d.THREAT_NASTY, d.THREAT_UNUSUAL]
                };
                let threat = [ff.TRIVIAL, ff.EASY, ff.TOUGH, ff.NASTY, ff.UNUSUAL];
                if let Some(k) = threat.iter().position(|&f| on(&fgw, f)) {
                    dc.push(ctx, rings[k]);
                }
                if yes(t.highlighted_summoner) {
                    dc.push(ctx, d.HALO_SUMMONER);
                }
            }
            // travel exclusion under the foreground while visible; a badge when unseen
            if on(bgw, bf.EXCL_CTR) {
                dc.push(ctx, d.TRAVEL_EXCLUSION_CENTRE_BG);
            } else if on(bgw, bf.TRAV_EXCL) {
                dc.push(ctx, d.TRAVEL_EXCLUSION_BG);
            }
        }
        if on(bgw, bf.REMEMBERED_INVIS) {
            dc.push(ctx, d.REMEMBERED_INVIS);
        }
    }

    // badges drawn above everything at the cell
    if on(bgw, bf.RAMPAGE) {
        dc.icons.extend(i.RAMPAGE);
    }
    if on(bgw, bf.UNSEEN) {
        if on(bgw, bf.EXCL_CTR) {
            dc.icons.extend(i.TRAVEL_EXCLUSION_CENTRE_FG);
        } else if on(bgw, bf.TRAV_EXCL) {
            dc.icons.extend(i.TRAVEL_EXCLUSION_FG);
        }
    }
    // cursors: the tutorial cursor takes precedence over the others
    if let Some(ck) = cell.flags.cursor_kind {
        let cid = match ck {
            CursorKind::Tutorial => i.TUTORIAL_CURSOR,
            CursorKind::Cursor1 => i.CURSOR,
            CursorKind::Cursor2 => i.CURSOR2,
            CursorKind::Cursor3 => i.CURSOR3,
        };
        dc.icons.extend(cid);
    }
    // travel trail: the step that led here and the step that leaves, as arrows
    let trail = t.travel_trail.unwrap_or(0);
    let (from, to) = (trail & 0xf, (trail & 0xf0) >> 4);
    if from != 0 {
        dc.icons.extend(plus(i.TRAVEL_PATH_FROM, from - 1));
    }
    if to != 0 {
        dc.icons.extend(plus(i.TRAVEL_PATH_TO, to - 1));
    }
    // and as headings of travel. Crawl's index is the delta from the other cell to this one,
    // counted 1 S, 2 SW, 3 W, 4 NW, 5 N, 6 NE, 7 E, 8 SE (tileview.cc _get_direction_index);
    // Dir8 counts 0 N clockwise, so index + 3 mod 8 for the way in, + 7 for the way on.
    if from != 0 || to != 0 {
        cell.trail = Some(Trail { from: (from != 0).then_some(((from + 3) % 8) as u8), to: (to != 0).then_some(((to + 7) % 8) as u8) });
    }

    cell.underlays = dc.under;
    cell.overlays = dc.over;
    cell.wall_shadows = dc.shadows;
    cell.shorelines = dc.shores;
    cell.wall_overlays = dc.wall;
    cell.translucent = dc.translucent;
    cell.icons = dc.icons;
}

// ---------------------------------------------------------------------------
// Billboards

/// The status icons a renderer shows on the sprite itself, by their tileinfo names.
fn icon_effect(name: &str) -> Option<SpriteEffect> {
    Some(match name {
        "PETRIFIED" | "PETRIFYING" => SpriteEffect::Petrified,
        "STICKY_FLAME" | "INNER_FLAME" => SpriteEffect::Burning,
        "POISON" | "MORE_POISON" | "MAX_POISON" => SpriteEffect::Poisoned,
        "CORRODED" => SpriteEffect::Corroded,
        "BERSERK" => SpriteEffect::Berserk,
        "GLOW_LIGHT" | "GLOW_HEAVY" => SpriteEffect::Glowing,
        "SUMMONED" => SpriteEffect::Summoned,
        "WATERLOGGED" => SpriteEffect::Wet,
        _ => return None,
    })
}

/// What the badges say has happened to the thing they are on, each once.
fn sprite_effects(badges: &[StatusIcon], gd: &Gamedata) -> Vec<SpriteEffect> {
    let mut out = Vec::new();
    for b in badges {
        if let Some(e) = gd.icons.name(b.tile).and_then(icon_effect) {
            if !out.contains(&e) {
                out.push(e);
            }
        }
    }
    out
}

/// A row of badges and how far the shifting part has moved (cell_renderer.js status_shift).
struct StatusRow {
    icons: Vec<StatusIcon>,
    shift: i32,
}

fn badge(tile: TileId, ox: f32) -> StatusIcon {
    StatusIcon { tile, ox, oy: 0.0, at_top: false, square: false, full: false }
}

/// Status badges for the thing in the foreground, laid out as the official client's
/// draw_foreground does: fixed-position marks first, then a row of status icons that shifts
/// left as it fills. Server `icons` use the server's own status-icon-sizes table: -1 not
/// drawn, 0 fixed, n shifts.
fn status_icons(t: &CellTiles, fgw: &FlagWord, bgw: &FlagWord, ctx: &Ctx) -> StatusRow {
    let (i, ff, gd) = (&ctx.i, &ctx.fg, ctx.gd);
    let mut out: Vec<StatusIcon> = Vec::new();
    let add = |out: &mut Vec<StatusIcon>, id: Option<TileId>, ox: i32| out.extend(id.map(|id| badge(id, ox as f32)));
    // the net and the web cover the whole cell
    for (f, id) in [(ff.NET, i.TRAP_NET), (ff.WEB, i.TRAP_WEB)] {
        if on(fgw, f) {
            out.extend(id.map(|tile| StatusIcon { full: true, ..badge(tile, 0.0) }));
        }
    }
    // "something under this square": one icon (SOMETHING_UNDER) up to 0.34, three by what
    // lies there (ITEM_STACK_1..3) in master. A mark about the square, not the thing on it.
    let under = if on(fgw, ff.S_UNDER) {
        Some(i.ITEM_STACK_1.or(i.SOMETHING_UNDER))
    } else if on(fgw, ff.S_UNDER_GOOD) {
        Some(i.ITEM_STACK_2.or(i.SOMETHING_UNDER))
    } else if on(fgw, ff.S_UNDER_ARTEFACT) {
        Some(i.ITEM_STACK_3.or(i.SOMETHING_UNDER))
    } else {
        None
    };
    if let Some(id) = under.flatten() {
        out.push(StatusIcon { square: true, ..badge(id, 0.0) });
    }
    if on(fgw, ff.PET) {
        add(&mut out, i.FRIENDLY, 0);
    } else if on(fgw, ff.GD_NEUTRAL) {
        add(&mut out, i.GOOD_NEUTRAL, 0);
    } else if on(fgw, ff.NEUTRAL) {
        add(&mut out, i.NEUTRAL, 0);
    }
    if on(bgw, ctx.bg.REMEMBERED_INVIS) {
        add(&mut out, i.UNSEEN_INVIS_REMEMBERED, 0);
    }
    let mut shift = 0;
    if on(fgw, ff.PARALYSED) {
        add(&mut out, i.PARALYSED, 0);
        shift += 12;
    } else if on(fgw, ff.STAB) {
        add(&mut out, i.STAB_BRAND, 0);
        shift += 12;
    } else if on(fgw, ff.MAY_STAB) {
        add(&mut out, i.UNAWARE, 0);
        shift += 7;
    } else if on(fgw, ff.FLEEING) {
        add(&mut out, i.FLEEING, 0);
        shift += 3;
    }
    if on(fgw, ff.POISON) {
        add(&mut out, i.POISON, -shift);
        shift += 5;
    } else if on(fgw, ff.MORE_POISON) {
        add(&mut out, i.MORE_POISON, -shift);
        shift += 5;
    } else if on(fgw, ff.MAX_POISON) {
        add(&mut out, i.MAX_POISON, -shift);
        shift += 5;
    }
    for &sent in t.icons.as_deref().unwrap_or(&[]) {
        let id = gd.server_tile(sent as TileId, "icons");
        let size = gd.status_icon_size(id);
        if size < 0 {
            continue;
        }
        if size == 0 {
            add(&mut out, Some(id), 0);
        } else {
            add(&mut out, Some(id), -shift);
            shift += size;
        }
    }
    // the damage bar: the icon paints it across the sprite's feet; Isorun pins it to the top
    // of the frame, over the head, where the player's own bars sit, so every health bar reads the same way
    let mdam = [
        (ff.MDAM_LIGHT, i.MDAM_LIGHTLY_DAMAGED),
        (ff.MDAM_MOD, i.MDAM_MODERATELY_DAMAGED),
        (ff.MDAM_HEAVY, i.MDAM_HEAVILY_DAMAGED),
        (ff.MDAM_SEV, i.MDAM_SEVERELY_DAMAGED),
        (ff.MDAM_ADEAD, i.MDAM_ALMOST_DEAD),
    ];
    if let Some(&(_, id)) = mdam.iter().find(|(f, _)| on(fgw, *f)) {
        out.extend(id.map(|tile| StatusIcon { at_top: true, ..badge(tile, 0.0) }));
    }
    if ctx.demon_tier {
        let tiers =
            [(ff.DEMON_1, i.DEMON_NUM1), (ff.DEMON_2, i.DEMON_NUM2), (ff.DEMON_3, i.DEMON_NUM3), (ff.DEMON_4, i.DEMON_NUM4), (ff.DEMON_5, i.DEMON_NUM5)];
        if let Some(&(_, id)) = tiers.iter().find(|(f, _)| on(fgw, *f)) {
            add(&mut out, id, 0);
        }
    }
    StatusRow { icons: out, shift }
}

/// The "new stairs" / "new transporter" badge, drawn unless the shifting status row has moved
/// (cell_renderer.js: `bg.NEW_STAIR && status_shift == 0`).
fn new_feature_badge(cell: &mut SceneCell, bgw: &FlagWord, ctx: &Ctx, status_shift: i32) {
    if status_shift != 0 {
        return;
    }
    let id = if on(bgw, ctx.bg.NEW_STAIR) {
        ctx.i.NEW_STAIR
    } else if on(bgw, ctx.bg.NEW_TRANSPORTER) {
        ctx.i.NEW_TRANSPORTER
    } else {
        None
    };
    cell.icons.extend(id);
}

fn layer(tile: TileId) -> Layer {
    Layer { tile, ..Layer::default() }
}

/// A plain main-range sprite, possibly over a base tile (mimics, submerged things), with the
/// parchment's overlays.
fn sprite_layers(t: &CellTiles, fg: TileId, parchment: bool, gd: &Gamedata) -> Vec<Layer> {
    let mut layers = Vec::new();
    let main = |id: i64| layer(gd.server_tile(id as TileId, "main"));
    if let Some(b) = t.base.filter(|&b| b != 0) {
        layers.push(main(b));
    }
    layers.push(layer(fg));
    if parchment {
        layers.extend(t.overlay1.filter(|&o| o != 0).map(main));
        layers.extend(t.overlay2.filter(|&o| o != 0).map(main));
    }
    layers
}

static TREE: Lazy<Regex> = Lazy::new(|| Regex::new("TREE|MANGROVE|PLANT|BUSH").unwrap());
static CORPSE: Lazy<Regex> = Lazy::new(|| Regex::new("^(CORPSE|FOOD_BONE)(_|$)").unwrap());

fn add_billboards(out: &mut Vec<Billboard>, mc: &MapCell, cell: &mut SceneCell, state: &GameState, ctx: &Ctx, player_on_level: bool) {
    let Some(t) = mc.t.as_ref() else { return };
    let (gd, r) = (ctx.gd, &ctx.r);
    let fgw = ctx.fg(t.fg);
    let bgw = ctx.bg(t.bg);
    let fg = fgw.value;
    let fg_idx = fg as i64;
    let is_player_cell = player_on_level && mc.x == state.player.pos.x && mc.y == state.player.pos.y;
    let alpha = t.trans.unwrap_or(false).then_some(0.55);
    if let Some(cloud) = cell.cloud.filter(|&c| c != 0) {
        out.push(Billboard::new(mc.x, mc.y, cloud, BillboardKind::Cloud, 0.9));
    }
    // main-range server overlays are beams and zaps in flight
    for &o in t.ov.as_deref().unwrap_or(&[]) {
        if o >= r.feat_max && o < r.main_max {
            out.push(Billboard::new(mc.x, mc.y, o as TileId, BillboardKind::Projectile, 0.5));
        }
    }
    let row = status_icons(t, &fgw, &bgw, ctx);
    let m = &ctx.m;
    let parchment = matches!((m.PARCHMENT_LOW, m.PARCHMENT_HIGH), (Some(lo), Some(hi)) if fg >= lo && fg <= hi);
    if is_player_cell {
        // the player is the camera; still expose the doll so a 2D renderer can draw it
        if let Some(first) = t.doll.as_ref().and_then(|d| d.first()) {
            let mut b = Billboard::new(mc.x, mc.y, gd.server_tile(first[0] as TileId, "player"), BillboardKind::Player, 0.7);
            b.layers = doll_layers(t, gd);
            b.effects = sprite_effects(&row.icons, gd);
            b.status_icons = row.icons;
            b.alpha = alpha;
            b.ref_id = Some(PLAYER_REF);
            out.push(b);
        }
        new_feature_badge(cell, &bgw, ctx, row.shift);
        return;
    }
    if fg == 0 {
        new_feature_badge(cell, &bgw, ctx, row.shift);
        cell.icons.extend(row.icons.iter().map(|b| b.tile));
        return;
    }
    // Monsters go on the scene whenever the server still reports one on the cell, visible
    // or remembered (sensed, detected), as the 2D client draws them. Renderers dim remembered
    // cells; the sidebar filters on visibility.
    if let Some(mon) = state.map.monster(mc) {
        let mut b = Billboard::new(mc.x, mc.y, fg, BillboardKind::Monster, monster_height(mon, fg, gd));
        b.name = Some(mon.name.clone());
        b.scenery = is_scenery(Some(&mon.name));
        b.attitude = Some(attitude_of(mon));
        b.threat = threat_of(mon);
        b.damage = Some(damage_level(&fgw, &ctx.fg));
        b.ref_id = mon.id;
        b.alpha = alpha;
        let doll = t.doll.as_ref().filter(|d| !d.is_empty());
        let mcache = t.mcache.as_ref().filter(|m| !m.is_empty());
        if fg_idx >= r.main_max && doll.is_some() {
            b.layers = doll_layers(t, gd);
        } else if let (true, Some(mcache)) = (fg_idx >= r.mcache_start, mcache) {
            b.layers = mcache.iter().map(|m| Layer { tile: gd.server_tile(m[0] as TileId, "player"), ox: m[1] as f32, oy: m[2] as f32, ymax: None }).collect();
        } else {
            let layers = sprite_layers(t, fg, parchment, gd);
            if layers.len() > 1 {
                b.layers = layers;
            }
        }
        b.effects = sprite_effects(&row.icons, gd);
        b.status_icons = row.icons;
        out.push(b);
        new_feature_badge(cell, &bgw, ctx, row.shift);
        return;
    }
    if fg_idx < r.main_max && fg_idx >= r.feat_max {
        // an item
        let name = gd.main.base_name(fg).unwrap_or("");
        let is_tree = TREE.is_match(name);
        // a corpse or a skeleton (tilepick.cc: a skeleton is FOOD_BONE, a humanoid's FOOD_BONE_HUMANOID)
        let lying = CORPSE.is_match(name);
        let height = if is_tree {
            0.95
        } else if lying {
            LYING_LENGTH
        } else {
            ITEM_HEIGHT
        };
        let mut b = Billboard::new(mc.x, mc.y, fg, BillboardKind::Item, height);
        b.scenery = is_tree;
        b.lying = lying;
        let layers = sprite_layers(t, fg, parchment, gd);
        if layers.len() > 1 {
            b.layers = layers;
        }
        b.effects = sprite_effects(&row.icons, gd);
        b.status_icons = row.icons;
        b.alpha = alpha;
        let words = item_tile_name(Some(name));
        b.name = (!words.is_empty()).then_some(words);
        b.rarity = item_rarity(name);
        out.push(b);
    }
    new_feature_badge(cell, &bgw, ctx, row.shift);
}

/// Doll parts with the mcache offsets the official client applies (cell_renderer.js
/// draw_dolls): each part whose tile has an mcache entry is drawn at that entry's offset.
fn doll_layers(t: &CellTiles, gd: &Gamedata) -> Vec<Layer> {
    let mcache: HashMap<i64, [i64; 3]> = t.mcache.iter().flatten().map(|m| (m[0], *m)).collect();
    t.doll
        .iter()
        .flatten()
        .map(|d| {
            let m = mcache.get(&d[0]);
            Layer {
                tile: gd.server_tile(d[0] as TileId, "player"),
                ox: m.map_or(0.0, |m| m[1] as f32),
                oy: m.map_or(0.0, |m| m[2] as f32),
                ymax: Some(d[1] as f32),
            }
        })
        .collect()
}

fn monster_height(mon: &Monster, tile: TileId, gd: &Gamedata) -> f32 {
    let hp = mon.typedata.as_ref().and_then(|t| t.avghp).unwrap_or(20);
    let name = mon.name.as_str();
    if is_vegetation(Some(name)) {
        return 0.95;
    }
    // low piles: an actor behind one should show over its top
    if is_rubble(name) {
        return 0.5;
    }
    let mut h: f32 = if hp >= 200 {
        0.85
    } else if hp >= 100 {
        0.75
    } else if hp >= 50 {
        0.65
    } else if hp >= 20 {
        0.55
    } else if hp >= 8 {
        0.5
    } else {
        0.4
    };
    if let Some(rect) = gd.tile(tile, None) {
        // tiny sprites (rats, bats) stay small even if hp says otherwise
        let px = rect.h as f32 / 32.0;
        h = h.min((px * 0.95).max(0.3));
    }
    h
}

/// monster_list.js: the wound classes the glyph-mode health box takes, from the MDAM flags.
fn damage_level(fgw: &FlagWord, ff: &FgFlags) -> WoundLevel {
    if on(fgw, ff.MDAM_LIGHT) {
        WoundLevel::LightlyDamaged
    } else if on(fgw, ff.MDAM_MOD) {
        WoundLevel::ModeratelyDamaged
    } else if on(fgw, ff.MDAM_HEAVY) {
        WoundLevel::HeavilyDamaged
    } else if on(fgw, ff.MDAM_SEV) {
        WoundLevel::SeverelyDamaged
    } else if on(fgw, ff.MDAM_ADEAD) {
        WoundLevel::AlmostDead
    } else {
        WoundLevel::Uninjured
    }
}
