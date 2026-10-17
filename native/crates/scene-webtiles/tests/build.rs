//! The port of build.test.ts: the classification rules, and `build_scene` on a recorded level
//! with the checked-in gamedata.

mod common;

use std::cmp::Ordering;
use std::collections::HashMap;

use common::{gd, messages, msg_type, replayed};
use isorun_gamedata::Gamedata;
use isorun_scene::{
    cell_key, Attitude, Billboard, BillboardKind, CellKind, DoorState, FaceTiles, Feature, HandItem, HandLayer, Scene, SceneCell, Sky, Stance, UpDown,
    Visibility,
};
use isorun_scene_webtiles::*;
use isorun_webtiles::{initial_state, map_key, reduce, CellMonster, GameState, InvItem, ItemTile, MapCell, Monster, TileLayer, TileRef, TileWord, TypeData};

fn full(st: &GameState) -> Scene {
    build_scene(st, gd(), BuildOptions::default())
}

fn chained(st: &GameState, previous: &Scene) -> Scene {
    build_scene(st, gd(), BuildOptions { previous: Some(previous), ..BuildOptions::default() })
}

fn bg_of(gd: &Gamedata, mc: &MapCell) -> Option<u32> {
    let w = mc.t.as_ref()?.bg?;
    Some(gd.bg_bits(w.lo, w.hi).value)
}

/// The map's cells in key order (row by row), so a test picks the same cells every run.
fn cells(st: &GameState) -> Vec<&MapCell> {
    let mut keys: Vec<&u32> = st.map.cells.keys().collect();
    keys.sort();
    keys.into_iter().map(|k| &st.map.cells[k]).collect()
}

fn first_floor(st: &GameState) -> MapCell {
    let gd = gd();
    cells(st).into_iter().find(|c| bg_of(gd, c).is_some_and(|v| v != 0 && (v as i64) < gd.ranges.floor_max)).unwrap().clone()
}

fn set_bg(mc: &mut MapCell, bg: u32) {
    mc.t.get_or_insert_with(Default::default).bg = Some(TileWord::new(bg, 0));
}

fn put(st: &mut GameState, mc: MapCell) {
    st.map.cells.insert(map_key(mc.x, mc.y), mc);
}

fn stairs(dir: UpDown) -> Feature {
    Feature::Stairs { dir, branch: false, exit: false }
}

// ---- gamedata loader (as this package's tests check it)

#[test]
fn loads_tile_modules_with_increasing_ranges() {
    let r = gd().ranges;
    assert!(r.floor_max > 0);
    assert!(r.wall_max > r.floor_max);
    assert!(r.feat_max > r.wall_max);
    assert!(r.main_max > r.feat_max);
    assert!(r.player_max > r.main_max);
    assert!(r.first_transparent > r.floor_max);
    assert!(r.first_transparent < r.wall_max);
}

#[test]
fn resolves_names_and_tile_rects() {
    let gd = gd();
    let down = gd.dngn.id("DNGN_STONE_STAIRS_DOWN").unwrap();
    assert_eq!(gd.dngn.name(down), Some("DNGN_STONE_STAIRS_DOWN"));
    let rect = gd.tile(down, None).unwrap();
    assert_eq!(rect.atlas, "feat");
    assert!(rect.w > 0);
    assert_eq!(gd.dngn.base_name(gd.dngn.id("FLOOR_GREY_DIRT_3").unwrap()), Some("FLOOR_GREY_DIRT"));
    let tree = gd.dngn.id("DNGN_TREE").unwrap();
    assert_eq!(tree as i64, gd.ranges.wall_max);
    assert_eq!(gd.dngn.name(tree), Some("DNGN_TREE"));
}

#[test]
fn decodes_flag_words_from_the_server_enums() {
    let gd = gd();
    let fg = gd.fg_bits(531768, 268435456);
    assert_eq!(fg.value, 531768 & 0xffff);
    assert!(fg.get("FLYING"));
    assert!(!fg.get("S_UNDER"));
    let bg = gd.bg_bits(947, 0);
    assert_eq!(bg.value, 947);
    assert!(!bg.get("UNSEEN"));
}

// ---- classification

#[test]
fn needs_two_of_three_signals() {
    assert_eq!(classify_feature(Some("DNGN_STONE_STAIRS_DOWN"), Some(">"), Some(13)), Some(stairs(UpDown::Down)));
    assert!(matches!(classify_feature(Some("DNGN_STONE_STAIRS_DOWN"), Some("."), Some(1)), Some(Feature::Stairs { .. })));
    assert_eq!(classify_feature(Some("DNGN_CLOSED_DOOR"), Some("+"), Some(5)), Some(Feature::Door { state: DoorState::Closed }));
    assert_eq!(classify_feature(Some("DNGN_ENTER_SHOP"), None, Some(15)), Some(Feature::Shop));
    assert_eq!(classify_feature(Some("DNGN_ALTAR_ZIN"), Some("_"), Some(15)), Some(Feature::Altar { god: Some("zin".into()) }));
    assert_eq!(classify_feature(Some("DNGN_ENTER_LAIR"), Some(">"), Some(14)), Some(Feature::Portal));
}

#[test]
fn the_d1_start_square_is_the_dungeon_exit_an_up_stair_not_a_portal() {
    // under the player the glyph is `@`, so only the name and the minimap category vote (tileweb.cc _send_cell)
    let text = std::fs::read_to_string(common::fixture_dir().join("cdi-git-newgame-d1-cell.json")).unwrap();
    let fx: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(fx["cell"]["g"], "@");
    assert_eq!(fx["cell"]["mf"], 12); // MF_STAIR_UP
    let exit = Some(Feature::Stairs { dir: UpDown::Up, branch: false, exit: true });
    assert_eq!(classify_feature(fx["bgName"].as_str(), fx["cell"]["g"].as_str(), fx["cell"]["mf"].as_i64()), exit);
    // seen from a distance the glyph is `<` (feature-data.h DCHAR_STAIRS_UP)
    assert_eq!(classify_feature(Some("DNGN_EXIT_DUNGEON"), Some("<"), Some(12)), exit);
    // branch exits are up stairs underfoot too
    assert_eq!(classify_feature(Some("DNGN_EXIT_LAIR"), Some("@"), Some(12)), Some(stairs(UpDown::Up)));
    assert_eq!(classify_feature(Some("DNGN_EXIT_LAIR"), Some("<"), Some(12)), Some(stairs(UpDown::Up)));
    // portal exits (PORTAL_EXIT: DCHAR_ARCH, MF_PORTAL) stay portals
    assert_eq!(classify_feature(Some("DNGN_EXIT_ABYSS"), Some("@"), Some(23)), Some(Feature::Portal));
}

#[test]
fn chooses_stances() {
    let down = stairs(UpDown::Down);
    let s = |n: &str, f: Option<&Feature>| stance_for(Some(n), f);
    // a way up stands; a way down lies in the floor, a block like a corpse
    assert_eq!(s("DNGN_STONE_STAIRS_DOWN", Some(&down)), Stance::Lying);
    assert_eq!(s("DNGN_STONE_STAIRS_DOWN_I", None), Stance::Lying);
    assert_eq!(s("DNGN_ENTER_LAIR", Some(&Feature::Stairs { dir: UpDown::Down, branch: true, exit: false })), Stance::Lying);
    assert_eq!(s("DNGN_STONE_STAIRS_UP_I", None), Stance::Upright);
    assert_eq!(s("DNGN_EXIT_DUNGEON", Some(&Feature::Stairs { dir: UpDown::Up, branch: false, exit: true })), Stance::Upright);
    assert_eq!(s("DNGN_ESCAPE_HATCH_DOWN", Some(&Feature::Hatch { dir: UpDown::Down })), Stance::Lying);
    assert_eq!(s("DNGN_ESCAPE_HATCH_UP", Some(&Feature::Hatch { dir: UpDown::Up })), Stance::Upright);
    assert_eq!(s("DNGN_ESCAPE_HATCH_DOWN", None), Stance::Lying);
    // what the player steps on or uses lies as a block too: traps (a shaft among them) and transporters
    assert_eq!(s("DNGN_TRAP_SHAFT", Some(&Feature::Hatch { dir: UpDown::Down })), Stance::Lying);
    assert_eq!(s("DNGN_TRAP_SHAFT", None), Stance::Lying);
    assert_eq!(s("DNGN_TRAP_ARROW", Some(&Feature::Trap { kind: None })), Stance::Lying);
    assert_eq!(s("DNGN_TRAP_WEB", None), Stance::Lying);
    assert_eq!(s("DNGN_TRANSPORTER", Some(&Feature::Transporter)), Stance::Lying);
    assert_eq!(s("DNGN_TELEPORTER", None), Stance::Lying);
    // a landing is where a transporter puts you, and decorations are the floor's own
    assert_eq!(s("DNGN_TRANSPORTER_LANDING", None), Stance::Decal);
    assert_eq!(s("DNGN_DECORATIVE_FLOOR", None), Stance::Decal);
    assert_eq!(s("DNGN_ALTAR_ZIN", Some(&Feature::Altar { god: None })), Stance::Upright);
    assert_eq!(s("DNGN_GRANITE_STATUE", None), Stance::Upright);
    assert_eq!(s("DNGN_TREE", None), Stance::Upright);
    assert_eq!(s("DNGN_MANGROVE", None), Stance::Upright);
    // the standards a decorative floor plants are flags on poles
    assert_eq!(s("DNGN_ENSIGN_BEOGH", None), Stance::Upright);
    assert_eq!(s("DNGN_ENSIGN_GEHENNA", None), Stance::Upright);
    assert_eq!(s("DNGN_ENSIGN_DARK", None), Stance::Upright);
}

#[test]
fn tells_a_feature_that_stands_on_the_dungeon_from_one_built_into_it() {
    let f = |n: &str, f: Option<Feature>| stands_free(Some(n), f.as_ref());
    // what stands in its cell: turned upright over a turning map
    assert!(f("DNGN_GRANITE_STATUE", None));
    assert!(f("DNGN_ALTAR_ZIN", Some(Feature::Altar { god: None })));
    assert!(f("DNGN_BLUE_FOUNTAIN", Some(Feature::Fountain { kind: "blue".into() })));
    assert!(f("DNGN_TREE", None));
    assert!(f("DNGN_ENSIGN_DARK", None));
    // built in: doors and gates in any state, Zot's wall-bedded runelights, stairs and hatches
    assert!(!f("DNGN_CLOSED_DOOR", Some(Feature::Door { state: DoorState::Closed })));
    assert!(!f("DNGN_OPEN_DOOR", Some(Feature::Door { state: DoorState::Open })));
    assert!(!f("DNGN_GATE_RUNED_LEFT", Some(Feature::Door { state: DoorState::Runed })));
    assert!(!f("DNGN_RUNELIGHT", None));
    assert!(!f("DNGN_STONE_STAIRS_DOWN", Some(stairs(UpDown::Down))));
    assert!(!f("DNGN_ESCAPE_HATCH_UP", Some(Feature::Hatch { dir: UpDown::Up })));
    assert!(!f("DNGN_ENTER_LAIR", Some(Feature::Stairs { dir: UpDown::Down, branch: true, exit: false })));
    // what lies on the floor stays lying: it is ground, not a thing in the cell
    assert!(!f("DNGN_TRAP_ARROW", Some(Feature::Trap { kind: None })));
    assert!(!f("DNGN_TRAP_SHAFT", None));
}

#[test]
fn treats_piles_of_debris_and_other_stationary_rubble_as_scenery() {
    assert!(is_scenery(Some("pile of debris")));
    assert!(is_scenery(Some("briar patch")));
    assert!(is_scenery(Some("bush")));
    assert!(!is_vegetation(Some("pile of debris")));
    assert!(!is_scenery(Some("orc warrior")));
    assert!(!is_scenery(Some("orb of destruction")));
}

#[test]
fn picks_level_presentation_by_branch() {
    assert_eq!(level_presentation("Shoals").sky, Sky::Open);
    assert_eq!(level_presentation("Abyss").sky, Sky::Dark);
    assert_eq!(level_presentation("Dungeon").sky, Sky::None);
}

// ---- build_scene on a recorded level

#[test]
fn stands_a_tree_upright_rather_than_laying_it_on_the_floor() {
    let gd = gd();
    let mut st = replayed();
    let mut floor = first_floor(&st);
    let tree = gd.dngn.id("DNGN_TREE_2").unwrap();
    floor.g = Some("\u{2663}".into());
    set_bg(&mut floor, tree);
    let (x, y) = (floor.x, floor.y);
    put(&mut st, floor);
    let scene = full(&st);
    let cell = scene.cell(x, y).unwrap();
    assert_eq!(cell.kind, CellKind::Feature);
    assert_eq!(cell.feature_tile, Some(tree));
    assert_eq!(cell.stance, Some(Stance::Upright));
    assert_eq!(cell.label.as_deref(), Some("tree"));
}

#[test]
fn names_the_water_shorelines_among_the_overlays() {
    let gd = gd();
    let mut st = replayed();
    let mut floor = first_floor(&st);
    let id = |n: &str| gd.dngn.id(n).unwrap();
    let (water, shore, wave, murky, deep, ink, shadow) =
        (id("DNGN_SHALLOW_WATER"), id("SHORE_N"), id("DNGN_WAVE_N"), id("MURKY_WAVE_NW"), id("WAVE_DEEP_N"), id("WAVE_INK_FULL"), id("DNGN_WALL_SHADOW"));
    // the runs stand in dc-floor.txt order, with the colour variations of the shallow border between its base tiles and the murky set
    assert!(murky > wave + 8);
    let green = wave + 8;
    // the shoals water tiles sit between the murky borders and the deep waves: floors, not shorelines
    let shoals = id("SHOALS_SHALLOW_WATER");
    let ov = [shore, wave, green, murky, shoals, deep, ink, shadow];
    set_bg(&mut floor, water);
    floor.t.as_mut().unwrap().ov = Some(ov.iter().map(|&o| o as i64).collect());
    let (x, y) = (floor.x, floor.y);
    put(&mut st, floor);
    let scene = full(&st);
    let cell = scene.cell(x, y).unwrap();
    for o in ov {
        assert!(cell.overlays.contains(&o));
    }
    assert_eq!(cell.shorelines, vec![shore, wave, green, murky, deep, ink]);
    assert!(!cell.shorelines.contains(&shoals));
    assert_eq!(cell.wall_shadows, vec![shadow]);
}

#[test]
fn names_the_travel_exclusion_marks_as_translucent_on_a_floor_and_on_a_wall_face() {
    let gd = gd();
    let mut st = replayed();
    let excl = gd.dngn.id("TRAVEL_EXCLUSION_BG").unwrap();
    let centre = gd.dngn.id("TRAVEL_EXCLUSION_CENTRE_BG").unwrap();
    let mut floor = first_floor(&st);
    let mut wall =
        cells(&st).into_iter().find(|c| bg_of(gd, c).is_some_and(|v| (v as i64) >= gd.ranges.floor_max && (v as i64) < gd.ranges.wall_max)).unwrap().clone();
    // enums.js bg_flags: TRAV_EXCL 0x00400000, EXCL_CTR 0x00800000
    let fv = bg_of(gd, &floor).unwrap();
    set_bg(&mut floor, fv | 0x0080_0000);
    let wv = bg_of(gd, &wall).unwrap();
    set_bg(&mut wall, wv | 0x0040_0000);
    let (f, w) = ((floor.x, floor.y), (wall.x, wall.y));
    put(&mut st, floor);
    put(&mut st, wall);
    let scene = full(&st);
    let f = scene.cell(f.0, f.1).unwrap();
    assert!(f.overlays.contains(&centre));
    assert_eq!(f.translucent, vec![centre]);
    let w = scene.cell(w.0, w.1).unwrap();
    assert!(w.wall_overlays.contains(&excl));
    assert_eq!(w.translucent, vec![excl]);
}

#[test]
fn names_a_shop_by_its_wares() {
    let gd = gd();
    let mut st = replayed();
    let mut floor = first_floor(&st);
    let shop = gd.dngn.id("SHOP_WEAPONS").unwrap();
    floor.mf = Some(15);
    set_bg(&mut floor, shop);
    let (x, y) = (floor.x, floor.y);
    put(&mut st, floor);
    let scene = full(&st);
    let cell = scene.cell(x, y).unwrap();
    assert_eq!(cell.feature, Some(Feature::Shop));
    // the shopkeeper's own name (shopping.cc `shop_name`) is never sent per square
    assert_eq!(cell.label.as_deref(), Some("weapon shop"));
}

#[test]
fn builds_a_scene_with_walls_floors_and_monsters() {
    let gd = gd();
    let mut st = initial_state();
    let mut best = 0;
    let mut scene = full(&st);
    for m in messages() {
        reduce(&mut st, m);
        if matches!(msg_type(m), "map" | "player") {
            let s = full(&st);
            let n = s.monsters_in_view().len();
            if n >= best {
                best = n;
                scene = s;
            }
        }
    }
    assert!(scene.player_on_level);
    let mut kinds: HashMap<CellKind, usize> = HashMap::new();
    for c in scene.cells.values() {
        *kinds.entry(c.kind).or_default() += 1;
    }
    assert!(kinds[&CellKind::Floor] > 50);
    assert!(kinds[&CellKind::Wall] > 20);
    let under = scene.cell_under().unwrap();
    assert_ne!(under.kind, CellKind::Unknown);
    assert_eq!(under.visibility, Visibility::Visible);
    let mons = scene.monsters_in_view();
    assert!(!mons.is_empty());
    assert!(mons.iter().all(|m| scene.cell(m.x, m.y).unwrap().visibility == Visibility::Visible));
    // the player's own doll is not a monster
    assert!(!mons.iter().any(|m| (m.x, m.y) == scene.player));
    assert!(scene.level.ceiling_tile.is_some());
    // a per-cell ceiling always names a wall type standing within reach of that cell
    for c in scene.cells.values() {
        let Some(lid) = c.ceiling_tile else { continue };
        let r = CEILING_REACH;
        let near = (-r..=r).any(|dy| {
            (-r..=r).any(|dx| scene.cell(c.x + dx, c.y + dy).is_some_and(|w| w.kind == CellKind::Wall && w.wall_tile.is_some_and(|t| gd.basetile(t) == lid)))
        });
        assert!(near);
    }
    // walls are never roofed
    assert!(!scene.cells.values().any(|c| c.occluder && c.ceiling_tile.is_some()));
}

fn lids(s: &Scene) -> Vec<(i32, i32, i64)> {
    let mut v: Vec<_> = s.cells.values().map(|c| (c.x, c.y, c.ceiling_tile.map_or(-1, |t| t as i64))).collect();
    v.sort();
    v
}

#[test]
fn reuses_the_lids_of_the_last_build_when_the_walls_stood_and_votes_again_when_they_moved() {
    let gd = gd();
    let mut st = initial_state();
    let mut scene = Scene::default();
    let mut record = BuildRecord::default();
    for m in messages() {
        reduce(&mut st, m);
        if matches!(msg_type(m), "map" | "player") {
            scene = build_scene(&st, gd, BuildOptions { previous: Some(&scene), dirty: None, record: Some(&mut record) });
        }
        if st.map.cells.len() > 800 {
            break;
        }
    }
    let fresh = full(&st);
    let reused = build_scene(&st, gd, BuildOptions { previous: Some(&scene), dirty: None, record: Some(&mut record) });
    assert_eq!(lids(&reused), lids(&fresh));
    assert!(fresh.cells.len() > 100);
    // a new wall: the lids are voted again, and match a build from nothing
    let wall_cell =
        cells(&st).into_iter().find(|c| bg_of(gd, c).is_some_and(|v| (v as i64) >= gd.ranges.floor_max && (v as i64) < gd.ranges.wall_max)).unwrap().clone();
    let mut other = cells(&st)
        .into_iter()
        .find(|c| bg_of(gd, c).is_some_and(|v| v != 0 && (v as i64) < gd.ranges.floor_max) && (c.x - wall_cell.x).abs() > 10)
        .unwrap()
        .clone();
    other.t.as_mut().unwrap().bg = wall_cell.t.as_ref().unwrap().bg;
    put(&mut st, other);
    let changed = build_scene(&st, gd, BuildOptions { previous: Some(&reused), dirty: None, record: Some(&mut record) });
    assert_eq!(lids(&changed), lids(&full(&st)));
}

#[test]
fn keeps_the_layout_revision_while_only_monsters_light_and_cursors_change() {
    let gd = gd();
    let mut st = initial_state();
    let mut scene = Scene::default();
    // play the recording until a monster is in view; every `map` or `player` is a build, as the session does it
    for m in messages() {
        reduce(&mut st, m);
        if matches!(msg_type(m), "map" | "player") {
            scene = chained(&st, &scene);
        }
        if msg_type(m) == "map" && scene.player_on_level && !scene.monsters_in_view().is_empty() {
            break;
        }
    }
    assert!(!scene.monsters_in_view().is_empty());
    assert!(scene.layout_revision > 0);
    assert!(scene.layout_revision <= scene.revision);
    // the same state again: a new revision, the same layout
    let again = chained(&st, &scene);
    assert_eq!(again.revision, scene.revision + 1);
    assert_eq!(again.layout_revision, scene.layout_revision);
    // a monster stepping onto another floor cell moves nothing in the level
    let mc = cells(&st).into_iter().find(|c| c.mon.is_some() && c.t.as_ref().is_some_and(|t| t.fg.is_some_and(|f| f.value() != 0))).unwrap().clone();
    let pos = st.player.pos;
    let spots: Vec<MapCell> = cells(&st)
        .into_iter()
        .filter(|c| bg_of(gd, c).is_some_and(|v| v != 0 && (v as i64) < gd.ranges.floor_max) && c.mon.is_none() && !(c.x == pos.x && c.y == pos.y))
        .cloned()
        .collect();
    let mut dest = spots[0].clone();
    dest.mon = mc.mon.clone();
    {
        let (from, to) = (mc.t.as_ref().unwrap(), dest.t.get_or_insert_with(Default::default));
        to.fg = from.fg;
        to.doll = from.doll.clone();
        to.mcache = from.mcache.clone();
    }
    let mut left = mc.clone();
    left.mon = None;
    {
        let t = left.t.as_mut().unwrap();
        t.fg = Some(TileWord::new(0, 0));
        t.doll = None;
        t.mcache = None;
    }
    put(&mut st, dest);
    put(&mut st, left);
    let moved = chained(&st, &again);
    assert_eq!(moved.revision, again.revision + 1);
    assert_eq!(moved.layout_revision, again.layout_revision);
    // a floor cell becoming a tree is the level changing
    let tree = gd.dngn.id("DNGN_TREE_2").unwrap();
    let mut floor = spots[1].clone();
    floor.g = Some("\u{2663}".into());
    set_bg(&mut floor, tree);
    put(&mut st, floor);
    let grown = chained(&st, &moved);
    assert_eq!(grown.layout_revision, grown.revision);
    // the player stepping moves nothing in the level either
    st.player.pos.x += 1;
    let stepped = chained(&st, &grown);
    assert_eq!(stepped.revision, grown.revision + 1);
    assert_eq!(stepped.layout_revision, grown.layout_revision);
}

#[test]
fn carries_what_the_official_client_paints_on_a_cell() {
    let gd = gd();
    let mut st = initial_state();
    let from = gd.icons.id("TRAVEL_PATH_FROM").unwrap();
    let to = gd.icons.id("TRAVEL_PATH_TO").unwrap();
    let blood = gd.dngn.id("BLOOD").unwrap();
    let blood_n = gd.tile_count(blood);
    let shadow = gd.dngn.id("DNGN_WALL_SHADOW").unwrap();
    let shadow_dark = gd.dngn.id("DNGN_WALL_SHADOW_DARK").unwrap();
    let shadow_n = gd.tile_count(shadow);
    assert_eq!(shadow_n, 7); // W, NW corner, N, NE corner, E, W with open NW, E with open NE (tilecell.cc)
    let (mut trail, mut bloody, mut shadows, mut badges) = (0, 0, 0, 0);
    for m in messages() {
        reduce(&mut st, m);
        if msg_type(m) != "map" {
            continue;
        }
        let scene = full(&st);
        for mc in st.map.cells.values() {
            let cell = scene.cell(mc.x, mc.y).unwrap();
            let t = mc.t.clone().unwrap_or_default();
            let tt = t.travel_trail.unwrap_or(0);
            if tt != 0 {
                // the step in and the step out, as arrow icons above everything
                if tt & 0xf != 0 {
                    assert!(cell.icons.contains(&(from + (tt & 0xf) as u32 - 1)));
                    assert_eq!(cell.trail.unwrap().from, Some((((tt & 0xf) + 3) % 8) as u8));
                }
                if tt & 0xf0 != 0 {
                    assert!(cell.icons.contains(&(to + ((tt & 0xf0) >> 4) as u32 - 1)));
                    assert_eq!(cell.trail.unwrap().to, Some(((((tt & 0xf0) >> 4) + 7) % 8) as u8));
                }
                trail += 1;
            } else {
                for &i in &cell.icons {
                    assert!(i < from || i >= to + 8);
                }
            }
            if t.bloody.unwrap_or(false) && cell.kind == CellKind::Floor {
                let seed = t.flv.as_ref().and_then(|f| f.s).unwrap_or(0);
                assert!(cell.overlays.contains(&(blood + (seed % blood_n as i64) as u32)));
                bloody += 1;
            }
            if t.ov.as_ref().is_some_and(|o| !o.is_empty()) && cell.kind == CellKind::Floor {
                // server overlays (wall shadows) survive on floor cells, and the shadows among them are named
                for &o in t.ov.as_ref().unwrap() {
                    if o <= 0 || o >= gd.ranges.feat_max {
                        continue;
                    }
                    let o = o as u32;
                    assert!(cell.overlays.contains(&o));
                    let is_shadow = (o >= shadow && o < shadow + shadow_n) || (o >= shadow_dark && o < shadow_dark + shadow_n);
                    assert_eq!(cell.wall_shadows.contains(&o), is_shadow);
                    if is_shadow {
                        shadows += 1;
                    }
                }
            }
        }
        badges += scene.billboards.iter().filter(|b| !b.status_icons.is_empty()).count();
    }
    assert!(trail > 0);
    assert!(bloody > 0);
    assert!(shadows > 0);
    assert!(badges > 0);
}

#[test]
fn draws_no_decoration_by_a_name_the_server_does_not_publish() {
    assert_eq!(missing_tile_names(gd()), Vec::<String>::new());
}

#[test]
fn reports_every_enum_the_server_disagrees_on() {
    // 0.34 predates the paged inventory flag the protocol table knows from master
    let e = server_enums(gd());
    assert_eq!(e.mismatches, vec!["enums.js has no menu_flag.PAGED_INVENTORY (client assumes 2097152)".to_string()]);
    assert_eq!(e.mf, MfTable::default());
}

#[test]
fn badges_the_something_under_flag_with_the_icon_the_server_has() {
    let gd = gd();
    let under = gd.icons.id("ITEM_STACK_1").or(gd.icons.id("SOMETHING_UNDER")).unwrap();
    let mut st = initial_state();
    let mut seen = 0;
    for m in messages() {
        reduce(&mut st, m);
        if msg_type(m) != "map" {
            continue;
        }
        let flagged = st
            .map
            .cells
            .values()
            .filter(|mc| {
                let w = mc.t.as_ref().and_then(|t| t.fg).unwrap_or_default();
                gd.fg_bits(w.lo, w.hi).get("S_UNDER")
            })
            .count();
        if flagged == 0 {
            continue;
        }
        for b in full(&st).billboards {
            seen += b.status_icons.iter().filter(|i| i.tile == under).count();
        }
    }
    assert!(seen > 0);
}

#[test]
fn lays_corpses_on_the_floor_and_stands_every_other_item_up() {
    let mut st = initial_state();
    let mut corpses = 0;
    for m in messages() {
        reduce(&mut st, m);
        if msg_type(m) != "map" {
            continue;
        }
        for b in full(&st).billboards {
            if b.kind != BillboardKind::Item {
                continue;
            }
            let corpse = b.name.as_deref().is_some_and(|n| n.ends_with(" corpse"));
            if corpse {
                corpses += 1;
            }
            assert_eq!(b.lying, corpse);
        }
    }
    // the black mamba corpse in cdi-0.34-watch.ndjson
    assert!(corpses > 0);
}

#[test]
fn marks_the_net_and_the_web_as_whole_cell_badges_and_every_corner_badge_as_not() {
    let gd = gd();
    let mut st = replayed();
    let mut me = st.map.cells[&map_key(st.player.pos.x, st.player.pos.y)].clone();
    // enums.js fg_flags: NET 0x00800000 and WEB 0x01000000 in the first word, POISON [0, 0x08000000] in the second (0.34)
    let w = me.t.as_ref().unwrap().fg.unwrap();
    let value = gd.fg_bits(w.lo, w.hi).value;
    me.t.as_mut().unwrap().fg = Some(TileWord::new(value | 0x0080_0000 | 0x0100_0000, 0x0800_0000));
    put(&mut st, me);
    let scene = full(&st);
    let player = scene.billboards.iter().find(|b| b.kind == BillboardKind::Player).unwrap();
    let full_cell: Vec<_> = player.status_icons.iter().filter(|i| i.full).map(|i| i.tile).collect();
    assert_eq!(full_cell, vec![gd.icons.id("TRAP_NET").unwrap(), gd.icons.id("TRAP_WEB").unwrap()]);
    let corner: Vec<_> = player.status_icons.iter().filter(|i| !i.full).map(|i| i.tile).collect();
    assert!(corner.contains(&gd.icons.id("POISON").unwrap()));
    assert!(player.effects.contains(&isorun_scene::SpriteEffect::Poisoned));
}

#[test]
fn pins_the_damage_bar_to_the_top_of_the_frame_and_leaves_every_other_badge_where_webtiles_puts_it() {
    let gd = gd();
    let mdam: Vec<u32> = ["MDAM_LIGHTLY_DAMAGED", "MDAM_MODERATELY_DAMAGED", "MDAM_HEAVILY_DAMAGED", "MDAM_SEVERELY_DAMAGED", "MDAM_ALMOST_DEAD"]
        .iter()
        .map(|n| gd.icons.id(n).unwrap())
        .collect();
    let mut st = initial_state();
    let (mut bars, mut others) = (0, 0);
    for m in messages() {
        reduce(&mut st, m);
        if msg_type(m) != "map" {
            continue;
        }
        for b in full(&st).billboards {
            for i in &b.status_icons {
                if mdam.contains(&i.tile) {
                    assert!(i.at_top);
                    assert_ne!(b.damage, Some(isorun_scene::WoundLevel::Uninjured));
                    bars += 1;
                } else {
                    assert!(!i.at_top);
                    others += 1;
                }
            }
        }
    }
    assert!(bars > 0);
    assert!(others > 0);
}

#[test]
fn gives_monsters_their_id_and_the_player_its_own() {
    let st = replayed();
    let scene = full(&st);
    for b in &scene.billboards {
        match b.kind {
            BillboardKind::Monster => assert_eq!(b.ref_id, monster_of(&st, b).and_then(|m| m.id)),
            BillboardKind::Player => assert_eq!(b.ref_id, Some(isorun_scene::PLAYER_REF)),
            _ => assert_eq!(b.ref_id, None),
        }
    }
}

// ---- monster list grouping (monster_list.js)

fn mon(name: &str, att: i64, avghp: i64, kind: i64) -> Monster {
    Monster {
        name: name.into(),
        plural: Some(format!("{name}s")),
        att,
        kind: Some(kind),
        typedata: Some(TypeData { avghp: Some(avghp), no_exp: None }),
        ..Monster::default()
    }
}

fn no_exp(mut m: Monster) -> Monster {
    m.typedata.as_mut().unwrap().no_exp = Some(true);
    m
}

fn named(mut m: Monster, clientid: i64) -> Monster {
    m.clientid = Some(clientid);
    m
}

#[test]
fn sorts_as_monster_info_less_than() {
    let rat = mon("rat", 0, 5, 10);
    let ogre = mon("ogre", 0, 40, 50);
    let friend = mon("ally", 4, 60, 70);
    assert_eq!(monster_sort(&ogre, &rat), Ordering::Less);
    assert_eq!(monster_sort(&rat, &friend), Ordering::Less);
    assert_eq!(monster_sort(&rat, &mon("rat", 0, 5, 10)), Ordering::Equal);
    // named monsters (with a client id) never combine, and come before unnamed ones of the same kind
    assert_eq!(monster_sort(&named(mon("rat", 0, 5, 10), 1), &named(mon("rat", 0, 5, 10), 2)), Ordering::Less);
    assert_eq!(monster_sort(&rat, &named(mon("rat", 0, 5, 10), 1)), Ordering::Greater);
}

#[test]
fn groups_equal_monsters_into_one_row_and_leaves_out_no_experience_monsters() {
    let list = [
        (1, mon("jackal", 0, 8, 20), true),
        (2, mon("jackal", 0, 8, 20), true),
        (3, mon("ogre", 0, 40, 50), true),
        (4, no_exp(mon("plant", 0, 10, 60)), true),
        (5, no_exp(mon("kraken tentacle", 0, 10, 61)), true),
        (6, mon("bat", 0, 3, 5), false),
    ];
    let mut st = initial_state();
    let mut scene = Scene { player_on_level: true, ..Scene::default() };
    for (x, m, visible) in list {
        let y = 1;
        let visibility = if visible { Visibility::Visible } else { Visibility::Remembered };
        scene.cells.insert(cell_key(x, y), SceneCell { x, y, kind: CellKind::Floor, visibility, occluder: false, floor_tile: 1, ..SceneCell::default() });
        let mut b = Billboard::new(x, y, 1, BillboardKind::Monster, 1.0);
        b.name = Some(m.name.clone());
        b.attitude = Some(Attitude::Hostile);
        scene.billboards.push(b);
        st.map.cells.insert(map_key(x, y), MapCell { x, y, mon: Some(CellMonster::Own(Box::new(m))), ..MapCell::default() });
    }
    let groups = monster_groups(&scene, &st);
    let names: Vec<Vec<&str>> = groups.iter().map(|g| g.iter().map(|b| b.name.as_deref().unwrap()).collect()).collect();
    assert_eq!(names, vec![vec!["ogre"], vec!["kraken tentacle"], vec!["jackal", "jackal"]]);
    assert!(is_excluded_from_list(&no_exp(mon("plant", 0, 1, 1))));
    assert!(!is_excluded_from_list(&no_exp(mon("active ballistomycete", 0, 1, 1))));
}

// ---- viewmodel

#[test]
fn resolves_the_wielded_weapon_and_the_off_hand_item_from_their_inventory_slots() {
    let mut st = initial_state();
    st.player.weapon_index = 0;
    st.player.offhand_index = 3;
    st.player
        .inv
        .insert(0, InvItem { slot: 0, name: Some("+0 war axe".into()), tile: Some(ItemTile::Layers(vec![TileLayer::Id(3672)])), ..InvItem::default() });
    st.player.inv.insert(
        3,
        InvItem {
            slot: 3,
            name: Some("+1 kite shield".into()),
            tile: Some(ItemTile::Layers(vec![
                TileLayer::Tex(TileRef { t: 4000, tex: 4, ymax: None }),
                TileLayer::Tex(TileRef { t: 4001, tex: 4, ymax: Some(20.0) }),
            ])),
            ..InvItem::default()
        },
    );
    let vm = viewmodel_for(&st, None);
    assert_eq!(vm.weapon, Some(HandItem { layers: vec![HandLayer { tile: 3672, layer: None, ymax: None }], name: Some("+0 war axe".into()) }));
    assert_eq!(
        vm.offhand,
        Some(HandItem {
            layers: vec![HandLayer { tile: 4000, layer: Some("4".into()), ymax: None }, HandLayer { tile: 4001, layer: Some("4".into()), ymax: Some(20.0) }],
            name: Some("+1 kite shield".into()),
        })
    );
}

#[test]
fn leaves_a_hand_empty_when_unarmed_when_the_server_publishes_no_off_hand_or_when_the_slot_has_no_tile() {
    let mut st = initial_state();
    let vm = viewmodel_for(&st, None);
    assert_eq!((vm.weapon, vm.offhand), (None, None));
    st.player.weapon_index = 2;
    st.player.inv.insert(2, InvItem { slot: 2, name: Some("a stick".into()), ..InvItem::default() });
    assert_eq!(viewmodel_for(&st, None).weapon, None);
}

#[test]
fn finds_the_watched_players_weapon_in_the_recorded_session_and_the_shield_from_the_paperdoll() {
    let gd = gd();
    let st = replayed();
    let vm = viewmodel_for(&st, Some(gd));
    let weapon = vm.weapon.unwrap();
    assert!(!weapon.layers.is_empty());
    assert_eq!(weapon.name, st.player.inv[&st.player.weapon_index].name);
    // 0.34 publishes no off-hand slot for armour; the doll's HAND2 part is the shield
    assert_eq!(st.player.offhand_index, -1);
    let off = vm.offhand.unwrap();
    assert!(off.name.as_deref().unwrap().starts_with("HAND2_"));
    assert_eq!(off.layers[0].layer, Some(gd.enums.get("texture", "PLAYER").unwrap().to_string()));
    // without gamedata nothing is guessed
    assert_eq!(viewmodel_for(&st, None).offhand, None);
}

// ---- item_tile_name

#[test]
fn reads_the_tile_the_server_sent_back_as_words() {
    let cases = [
        ("WPN_DAGGER", "dagger"),
        ("WPN_DAGGER_MAGIC", "dagger"),
        ("WPN_DAGGER_RANDART", "dagger"),
        ("WPN_GREAT_MACE", "great mace"),
        ("ARM_ROBE_RANDART_1", "robe"),
        ("MI_ARROW_STEEL_MAGIC", "steel arrow"),
        ("MI_DART_POISONED", "poisoned dart"),
        ("MI_LARGE_ROCK0", "large rock"),
        ("MI_THROWING_NET3", "throwing net"),
        ("GOLD04", "gold"),
        ("FOOD_RATION_2", "ration"),
        ("MISC_LAMP_OF_FIRE_INERT", "lamp of fire"),
        ("CORPSE_BAT", "bat corpse"),
        ("GEM_ORC_1", "gem"),
        ("TALISMAN_SNAKE", "talisman"),
        ("STAFF_OFFSET_2", "staff"),
        ("WAND_FLAME", "wand of flame"),
        ("WAND_OFFSET_3", "wand"),
        ("SCR_IDENTIFY", "scroll of identify"),
        ("SCROLL", "scroll"),
        ("AMU_THE_GOURMAND", "amulet of the gourmand"),
        ("AMU_NORMAL_OFFSET_12", "amulet"),
        ("RING_RANDART_OFFSET_2", "ring"),
        ("RING_PROTECTION_FROM_FIRE", "ring of protection from fire"),
        ("POTION_OFFSET_5", "potion"),
        ("UNSEEN_POTION", "potion"),
        ("BOOK_17", "book"),
        ("PARCHMENT_SINGLE_FIRE_HIGH", "parchment"),
        ("ARM_LEATHER_ARMOUR", "leather armour"),
    ];
    for (tile, words) in cases {
        assert_eq!(item_tile_name(Some(tile)), words, "{tile}");
    }
    assert_eq!(item_tile_name(None), "");
}

#[test]
fn names_every_item_tile_in_the_fixture_gamedata_from_its_base() {
    let gd = gd();
    let re = regex::Regex::new("^[a-z][a-z0-9 ]*$").unwrap();
    for id in gd.ranges.feat_max..gd.ranges.main_max {
        let Some(n) = gd.main.base_name(id as u32) else { continue };
        let w = item_tile_name(Some(n));
        assert!(re.is_match(&w), "{n}: {w}");
    }
    assert_eq!(item_tile_name(gd.main.base_name(gd.main.id("WPN_DAGGER_MAGIC").unwrap())), "dagger");
}

// ---- gate faces

#[test]
fn shows_each_side_of_a_row_of_doors_the_gate_piece_a_player_standing_there_sees() {
    let gd = gd();
    let id = |n: &str| gd.dngn.id(&format!("DNGN_{n}")).unwrap();
    let (left, mid, right) = (id("GATE_CLOSED_LEFT"), id("GATE_CLOSED_MIDDLE"), id("GATE_CLOSED_RIGHT"));
    let ns = |n| Some(FaceTiles { n: Some(n), ..FaceTiles::default() });
    let we = |w, e| Some(FaceTiles { w: Some(w), e: Some(e), ..FaceTiles::default() });
    // east–west: from the south as drawn, from the north the ends trade places
    assert_eq!(gate_faces(gd, Some("DNGN_GATE_CLOSED_LEFT")), ns(right));
    assert_eq!(gate_faces(gd, Some("DNGN_GATE_CLOSED_MIDDLE")), None);
    assert_eq!(gate_faces(gd, Some("DNGN_GATE_CLOSED_RIGHT")), ns(left));
    // north–south: the top-down pieces become the upright ones on either side
    assert_eq!(gate_faces(gd, Some("DNGN_VGATE_CLOSED_UP")), we(left, right));
    assert_eq!(gate_faces(gd, Some("DNGN_VGATE_CLOSED_MIDDLE")), we(mid, mid));
    assert_eq!(gate_faces(gd, Some("DNGN_VGATE_CLOSED_DOWN")), we(right, left));
    // every set keeps to its own art: runed, clear, crypt
    assert_eq!(gate_faces(gd, Some("DNGN_VGATE_RUNED_CLEAR_UP")), we(id("GATE_RUNED_CLEAR_LEFT"), id("GATE_RUNED_CLEAR_RIGHT")));
    assert_eq!(gate_faces(gd, Some("DNGN_VGATE_CLOSED_DOWN_CRYPT")), we(id("GATE_CLOSED_RIGHT_CRYPT"), id("GATE_CLOSED_LEFT_CRYPT")));
    assert_eq!(gate_faces(gd, Some("DNGN_CLOSED_DOOR")), None);
}
