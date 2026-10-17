//! The port of vault.test.ts: the parser, and the scene with the checked-in gamedata as the
//! tile set.

use std::collections::HashMap;
use std::path::PathBuf;

use isorun_gamedata::Gamedata;
use isorun_scene::{is_walkable, Attitude, Beacon, BillboardKind, CellKind, DoorState, Feature, Rect, Sky, Stance, TileId, UpDown, Visibility, WallStyle};
use isorun_vault::*;
use once_cell::sync::Lazy;

const VERSION: &str = "acd3d60e20f899c1c8a546953d6ffa0f6c7fe0c8";

static GD: Lazy<Gamedata> = Lazy::new(|| {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packages/scene-webtiles/test/fixtures/gamedata").join(VERSION);
    let files: HashMap<String, String> =
        isorun_gamedata::script_files().into_iter().filter_map(|f| std::fs::read_to_string(dir.join(f)).ok().map(|t| (f.to_string(), t))).collect();
    Gamedata::from_files(&files, VERSION).unwrap()
});

/// The tile set a Gamedata gives a vault: names across every module, and the variant count.
struct Names(&'static Gamedata);

impl TileNames for Names {
    fn id(&self, n: &str) -> Option<TileId> {
        self.0.dngn.id(n).or_else(|| self.0.main.id(n)).or_else(|| self.0.player.id(n))
    }
    fn tile_count(&self, id: TileId) -> u32 {
        self.0.tile_count(id)
    }
}

const SMALL: &str = "
NAME: small
TILE: # = wall_brick_dark
FTILE: . = floor_grey_dirt
KFEAT: > = dngn_stone_stairs_down
KFEAT: + = dngn_closed_door
KITEM: $ = gold05
KMONS: P = mons_plant
EYE: @ S
MAP
#######
#..>..#
#.@.P.#
#..$..#
###+###
#.....#
#######
ENDMAP
";

fn err(text: &str) -> String {
    parse_des(text).unwrap_err().to_string()
}

#[test]
fn reads_the_map_the_legend_the_eye_and_its_heading() {
    let v = parse_des(SMALL).unwrap();
    assert_eq!(v.name, "small");
    assert_eq!((v.width, v.height), (7, 7));
    assert_eq!(v.eye, Eye { x: 2, y: 2, facing: 4 });
    assert_eq!(v.light, (2, 2));
    assert_eq!(vault_tile_names(&v), ["WALL_BRICK_DARK", "FLOOR_GREY_DIRT", "DNGN_STONE_STAIRS_DOWN", "DNGN_CLOSED_DOOR", "GOLD05", "MONS_PLANT"]);
}

#[test]
fn pads_short_rows_with_void_and_drops_blank_rows_around_the_map() {
    let v = parse_des("FTILE: . = floor_grey_dirt\nTILE: # = wall_brick_dark\nEYE: @\nMAP\n\n###\n#@#\n#####\n###\n\nENDMAP\n").unwrap();
    assert_eq!(v.rows(), ["###  ", "#@#  ", "#####", "###  "]);
}

#[test]
fn refuses_every_directive_outside_the_subset_naming_the_line() {
    assert!(err("NAME: x\nSUBST: . = .:9 ,\nMAP\n@\nENDMAP\n").starts_with("line 2: SUBST is not in the supported subset"));
    assert!(err("TAGS: no_monster_gen\n").starts_with("line 1: TAGS"));
    assert!(err("MARKER: > = lua:foo\n").starts_with("line 1: MARKER"));
    assert!(err("nonsense here\n").starts_with("line 1: not a directive"));
    assert_eq!(DIRECTIVES, ["NAME", "TILE", "FTILE", "KFEAT", "KITEM", "KMONS", "EYE", "LIGHT", "MAP", "ENDMAP"]);
}

#[test]
fn names_the_line_of_every_mistake_in_the_map_or_the_legend() {
    let base = "TILE: # = wall_brick_dark\nFTILE: . = floor_grey_dirt\nEYE: @\n";
    let with = |rest: &str| format!("{base}{rest}");
    assert!(err(&with("MAP\n###\n#@#\n###\n")).contains("line 4: MAP without ENDMAP"));
    assert!(err(&with("MAP\n###\n#x#\n###\nENDMAP\n")).contains("line 6: \"x\" at 1,1 of the map has no legend entry"));
    // a glyph with only a floor tile is open floor of that tile
    let pool = parse_des(&with("FTILE: w = dngn_deep_water\nMAP\n####\n#@w#\n####\nENDMAP\n")).unwrap();
    assert_eq!(pool.rows()[1], "#@w#");
    let w = pool.legend.iter().find(|e| e.glyph == 'w').unwrap();
    assert_eq!((w.kind, w.tile.as_str()), (LegendKind::Floor, "DNGN_DEEP_WATER"));
    assert!(err(&with("MAP\n#####\n#@.@#\n#####\nENDMAP\n")).contains("a second \"@\" (the eye)"));
    assert!(err(&with("MAP\n###\n#.#\n###\nENDMAP\n")).contains("the eye's glyph \"@\" is not on the map"));
    assert!(err("TILE: # = wall_brick_dark\nEYE: @\nMAP\n###\n#@#\n###\nENDMAP\n").contains("no floor: add \"FTILE: . = <tile>\""));
    assert!(err(&with("TILE: # = wall_stone_dark\nMAP\n#@#\nENDMAP\n")).contains("line 4: \"#\" is already defined on line 1"));
    assert!(err(&with("KFEAT: @ = dngn_stone_stairs_down\nMAP\n###\n#@#\n###\nENDMAP\n")).contains("\"@\" is the eye and cannot also be a feat"));
    assert!(err(&with("EYE: x\n")).contains("line 4: a second EYE"));
    assert!(err("EYE: @ UP\n").contains("heading UP is not one of"));
    assert!(err("KFEAT: > dngn_stone_stairs_down\n").contains("KFEAT reads \"KFEAT: <glyph> = <tile>\""));
    assert!(err("KFEAT: > = not a name\n").contains("KFEAT reads"));
}

#[test]
fn is_an_error_type_with_the_line_on_it() {
    let e = parse_des("WEIGHT: 5\n").unwrap_err();
    assert_eq!(e.line, 1);
    let _: &dyn std::error::Error = &e;
}

#[test]
fn builds_walls_features_doors_items_and_monsters_from_the_tile_set_seen_and_lit_once() {
    let gd: &'static Gamedata = &GD;
    let v = parse_des(SMALL).unwrap();
    let s = vault_scene(&v, &Names(gd)).unwrap();
    assert_eq!(s.bounds, Rect { left: 0, top: 0, right: 6, bottom: 6 });
    assert_eq!(s.cells.len(), 49);
    let wall = s.cell(0, 0).unwrap();
    assert_eq!(wall.kind, CellKind::Wall);
    assert!(wall.occluder);
    assert!(gd.dngn.base_name(wall.wall_tile.unwrap()).unwrap().starts_with("WALL_BRICK_DARK"));
    assert_eq!(wall.wall_style, Some(WallStyle::Solid));
    let floor = s.cell(1, 1).unwrap();
    assert_eq!(floor.kind, CellKind::Floor);
    assert!(is_walkable(Some(floor)));
    assert_eq!(floor.visibility, Visibility::Visible);
    assert!(gd.dngn.base_name(floor.floor_tile).unwrap().starts_with("FLOOR_GREY_DIRT"));
    let stairs = s.cell(3, 1).unwrap();
    assert_eq!(stairs.kind, CellKind::Feature);
    assert_eq!(stairs.feature, Some(Feature::Stairs { dir: UpDown::Down, branch: false, exit: false }));
    assert_eq!(stairs.stance, Some(Stance::Lying));
    assert_eq!(stairs.beacon, Some(Beacon::Down));
    assert_eq!(stairs.feature_tile, gd.dngn.id("DNGN_STONE_STAIRS_DOWN"));
    let door = s.cell(3, 4).unwrap();
    assert_eq!(door.kind, CellKind::Door);
    assert!(door.occluder);
    assert_eq!(door.feature, Some(Feature::Door { state: DoorState::Closed }));
    assert_eq!(s.billboards.len(), 2);
    let plant = s.billboards.iter().find(|b| b.kind == BillboardKind::Monster).unwrap();
    assert_eq!((plant.x, plant.y, plant.scenery, plant.attitude, plant.height), (4, 2, true, Some(Attitude::Hostile), 0.95));
    assert_eq!(plant.name.as_deref(), Some("plant"));
    assert!(gd.player.base_name(plant.tile).unwrap().starts_with("MONS_PLANT"));
    let gold = s.billboards.iter().find(|b| b.kind == BillboardKind::Item).unwrap();
    assert_eq!((gold.x, gold.y, gold.height, Some(gold.tile)), (3, 3, 0.4, gd.main.id("GOLD05")));
    assert_eq!(s.player, (2, 2));
    assert!(s.player_on_level);
    assert_eq!(s.level.sky, Sky::None);
    assert!(gd.dngn.base_name(s.level.ceiling_tile.unwrap()).unwrap().starts_with("WALL_BRICK_DARK"));
    assert_eq!((s.revision, s.layout_revision), (1, 1));
    let cam = vault_camera(&v);
    assert_eq!((cam.x, cam.y, cam.facing, cam.yaw), (2, 2, 4, std::f64::consts::PI));
}

#[test]
fn gives_floors_and_walls_their_variants_by_position_the_same_every_build() {
    assert_eq!(variant_at(100, 1, 3, 4), 100);
    let a = variant_at(100, 8, 3, 4);
    assert!((100..108).contains(&a));
    assert_eq!(variant_at(100, 8, 3, 4), a);
    let mut seen = std::collections::HashSet::new();
    for x in 0..8 {
        for y in 0..8 {
            seen.insert(variant_at(100, 8, x, y));
        }
    }
    assert!(seen.len() > 3);
    // the TS client's numbers, its 32-bit integer arithmetic and all
    assert_eq!([variant_at(100, 8, 3, 4), variant_at(1104, 12, 31, 14), variant_at(6863, 3, 3, 11), variant_at(40000, 7, 511, 300)], [106, 1107, 6865, 40002]);
}

#[test]
fn refuses_a_tile_the_tile_set_lacks_and_a_room_open_to_void() {
    let names = Names(&GD);
    let scene_of = |text: &str| vault_scene(&parse_des(text).unwrap(), &names);
    assert!(scene_of(&SMALL.replace("gold05", "gold99")).unwrap_err().message.contains("tiles not in this tile set: GOLD99"));
    let open = |text: &str| scene_of(text).unwrap_err().message.starts_with("the room is open at 5,");
    // a floor cell on the edge touches void
    assert!(open("TILE: # = wall_brick_dark\nFTILE: . = floor_grey_dirt\nEYE: @\nMAP\n#####\n#.@..\n#####\nENDMAP\n"));
    // a tree may stand at the edge of the void: the wood is the boundary
    assert!(scene_of("TILE: # = wall_brick_dark\nFTILE: . = floor_grey_dirt\nKFEAT: T = dngn_tree\nEYE: @\nMAP\n#####\n#.@T\n#####\nENDMAP\n").is_ok());
    assert!(open("TILE: # = wall_brick_dark\nFTILE: . = floor_grey_dirt\nKFEAT: T = dngn_tree\nEYE: @\nMAP\n#####\n#.@T.\n#####\nENDMAP\n"));
    // the eye's and the light's glyphs are floor by construction; a wall beside void is fine
    assert!(scene_of("TILE: # = wall_brick_dark\nFTILE: . = floor_grey_dirt\nEYE: @\nLIGHT: l\nMAP\n#####\n#.@.#\n##l##\n#####\nENDMAP\n").is_ok());
}

#[test]
fn takes_any_name_lookup_as_a_tile_set() {
    let v = parse_des(SMALL).unwrap();
    let ids = |n: &str| GD.dngn.id(n).or_else(|| GD.main.id(n)).or_else(|| GD.player.id(n));
    let s = vault_scene(&v, &ids).unwrap();
    // with no variant counts every cell takes the named tile
    assert_eq!(s.cell(1, 1).unwrap().floor_tile, GD.dngn.id("FLOOR_GREY_DIRT").unwrap());
}
