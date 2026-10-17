//! A real server's gamedata, checked against what the JavaScript client makes of it (the
//! values were read off packages/gamedata's `loadGamedata` running on the same files).

mod common;

use isorun_gamedata::{verify_enums, Gamedata, TileRect};
use serde_json::json;

fn load(dir: &std::path::Path) -> Gamedata {
    let gd = Gamedata::from_files(&common::read_dir(dir), "test").unwrap();
    assert!(gd.diagnostics.is_empty(), "{:?}", gd.diagnostics);
    gd
}

fn rect(atlas: &str, sx: u32, sy: u32, w: u32, h: u32, ox: i32, oy: i32) -> TileRect {
    TileRect {
        atlas: atlas.into(),
        sx,
        sy,
        w,
        h,
        ox,
        oy,
        cell: 32,
    }
}

fn sorted(mut v: Vec<&str>) -> Vec<&str> {
    v.sort();
    v
}

fn check(gd: &Gamedata) {
    let r = gd.ranges;
    assert_eq!(
        [
            r.floor_max,
            r.wall_max,
            r.feat_max,
            r.main_max,
            r.player_max,
            r.gui_max,
            r.icons_max
        ],
        [1104, 2668, 3454, 5215, 8883, 10173, 10345]
    );
    assert_eq!((r.first_transparent, r.mcache_start), (1264, 7578));
    assert_eq!(gd.fingerprint, "9d8e177a");
    assert!(gd.modules().iter().all(|m| m.offset == 0));

    // names, aliases (`val = exports.FLOOR_NORMAL = exports.FLOOR_GREY_DIRT; val++;`) and bases
    assert_eq!(gd.dngn.id("FLOOR_GREY_DIRT"), Some(2));
    assert_eq!(gd.dngn.id("FLOOR_NORMAL"), Some(2));
    assert_eq!(gd.dngn.name(2), Some("FLOOR_GREY_DIRT"));
    assert_eq!(gd.dngn.id("FLOOR_GREY_DIRT_3"), Some(5));
    assert_eq!(gd.dngn.base_name(5), Some("FLOOR_GREY_DIRT"));
    assert_eq!(gd.basetile(5), 2);
    assert_eq!(gd.dngn.id("WALL_NORMAL"), Some(1104));
    assert_eq!(gd.dngn.id("WALL_BRICK_DARK"), Some(1104));
    assert_eq!(gd.dngn.name(1104), Some("WALL_BRICK_DARK_1"));
    // a module's *_MAX sentinel names the next module's first tile only when nothing else does
    assert_eq!(gd.dngn.id("TILE_WALL_MAX"), Some(2668));
    assert_eq!(gd.dngn.name(2668), Some("DNGN_TREE"));
    assert_eq!(gd.dngn.base_name(2670), Some("DNGN_TREE"));
    assert_eq!(gd.main.id("CORPSE_RAT"), Some(4667));
    assert_eq!(gd.player.id("MONS_GOBLIN"), Some(7209));
    assert_eq!(gd.player.id("BASE_HUMAN"), Some(7596));
    assert_eq!(gd.gui.id("CMD_EXPLORE"), Some(9509));
    assert_eq!(gd.icons.id("CURSOR"), Some(10181));
    assert_eq!(gd.main.name(2), None);

    // tile rects and counts
    assert_eq!(gd.tile(2, None), Some(rect("floor", 64, 0, 32, 32, 0, 0)));
    assert_eq!(gd.tile_count(2), 8);
    assert_eq!(gd.tile(5, None), Some(rect("floor", 160, 0, 32, 32, 0, 0)));
    assert_eq!(gd.tile_count(5), 1);
    assert_eq!(gd.tile(1104, None), Some(rect("wall", 0, 0, 32, 32, 0, 0)));
    assert_eq!(gd.tile_count(1104), 12);
    assert_eq!(gd.tile(2668, None), Some(rect("feat", 0, 0, 30, 32, 0, 0)));
    assert_eq!(gd.tile_count(2668), 13);
    assert_eq!(gd.tile(2670, None), Some(rect("feat", 62, 0, 32, 31, 0, 1)));
    assert_eq!(
        gd.tile(4667, None),
        Some(rect("main", 254, 908, 32, 21, 0, 11))
    );
    assert_eq!(
        gd.tile(7209, None),
        Some(rect("player", 722, 1670, 28, 25, 2, 7))
    );
    assert_eq!(
        gd.tile(7596, None),
        Some(rect("player", 157, 2054, 22, 30, 5, 2))
    );
    assert_eq!(gd.tile_count(7596), 6);
    assert_eq!(
        gd.tile(9509, None),
        Some(rect("gui", 476, 518, 28, 28, 2, 2))
    );
    assert_eq!(
        gd.tile(10181, None),
        Some(rect("icons", 254, 0, 32, 32, 0, 0))
    );
    assert_eq!(gd.tile(10345, None), None);
    assert_eq!(gd.tile_count(10345), 1);

    // by layer: an atlas name, or a texture index with the server's id
    assert_eq!(gd.tile(2668, Some("feat")), gd.tile(2668, None));
    assert_eq!(gd.tile(2668, Some("wall")), None);
    assert_eq!(gd.enums.get("texture", "FEAT"), Some(2));
    assert_eq!(gd.tile(2668, Some("2")), gd.tile(2668, None));
    assert_eq!(gd.tile_for_texture(7596, 3), gd.tile(7596, None));
    assert_eq!(gd.tile_for_texture(4667, 4), gd.tile(4667, None));
    assert_eq!(gd.module_for_texture(4).map(|m| m.atlas), Some("main"));

    // a tile drawn at an odd width sits half a texel off centre, rounded down
    let feat = gd.module("feat").unwrap();
    let odd = (feat.first..feat.end)
        .find(|&id| feat.tile_info(id).is_some_and(|i| i.w == 31 && i.ox == 0))
        .unwrap();
    assert_eq!(gd.tile(odd as u32, None).unwrap().ox, 0);

    // status icons, from status-icon-sizes.js's switch
    assert!(gd.has_status_icon_sizes());
    let size = |n: &str| gd.status_icon_size(gd.icons.id(n).unwrap());
    assert_eq!(
        [
            size("BERSERK"),
            size("CONSTRICTED"),
            size("DRAIN"),
            size("CURSOR")
        ],
        [0, 11, 6, -1]
    );
    assert_eq!(gd.status_icon_size(5), -1);

    // flag words, as prepare_fg_flags / prepare_bg_flags decode them
    let fg = gd.fg(&json!(0x10001 + 0x80000));
    assert_eq!(
        (fg.value, sorted(fg.set_flags().collect())),
        (1, vec!["FLYING", "PET"])
    );
    let fg = gd.fg(&json!([
        0x00100000 | 42 | 0x40000000,
        0x18000000 | 0x40000000
    ]));
    assert_eq!(
        (fg.value, sorted(fg.set_flags().collect())),
        (42, vec!["EASY", "MAX_POISON", "MDAM_LIGHT", "STAB"])
    );
    let fg = gd.fg(&json!([0x80000007u32, 0x0E | 0x20 | 0x00100000]));
    assert_eq!(
        (fg.value, sorted(fg.set_flags().collect())),
        (7, vec!["DEMON_1", "GHOST", "MDAM_MOD"])
    );
    assert!(fg.get("GHOST") && !fg.get("PET") && !fg.get("NO_SUCH_FLAG"));
    assert_eq!(gd.fg_bits(0x80000007, 0x0E | 0x20 | 0x00100000), fg);
    let bg = gd.bg(&json!(0x04000000 | 0x00180000 | 12));
    assert_eq!(
        (bg.value, sorted(bg.set_flags().collect())),
        (12, vec!["CURSOR1", "WATER"])
    );
    let bg = gd.bg(&json!([0x80040005u32, 0x21]));
    assert_eq!(
        (bg.value, sorted(bg.set_flags().collect())),
        (5, vec!["KRAKEN_SE", "KRAKEN_SW", "RAMPAGE", "UNSEEN"])
    );
    assert_eq!(gd.bg(&json!(null)).value, 0);
    assert!(gd.fg_flag_names().iter().any(|n| n == "MDAM_ADEAD"));

    // the enum tables
    assert_eq!(gd.enums.get("", "gxm"), Some(80));
    assert_eq!(gd.enums.get("mouse_mode", "TARGET"), Some(2));
    assert_eq!(gd.enums.get("menu_flag", "ARROWS_SELECT"), Some(0x40000));
    assert_eq!(gd.enums.get("texture", "DEFAULT"), Some(4));
    assert_eq!(gd.enums.get("", "HALO_UMBRA_LAST"), Some(5));
    assert_eq!(gd.enums.get("CHATTR", "ATTRMASK"), Some(15));
    assert_eq!(gd.enums.get("", "MF_SKIP"), Some(25));
    assert_eq!(gd.enums.get("ui", "VIEW_MAP"), Some(2));
    assert!(gd.enums.value("", "prepare_fg_flags").is_none());

    assert!(verify_enums(
        gd,
        &[
            ("texture", &[("FLOOR", 0), ("ICONS", 6)]),
            ("", &[("MF_WALL", 2)])
        ]
    )
    .is_empty());
    assert_eq!(
        verify_enums(
            gd,
            &[
                ("texture", &[("FLOOR", 1), ("NOPE", 3)]),
                ("no_such_group", &[("A", 1)]),
                ("", &[("mouse_mode", 0)]),
            ]
        ),
        [
            "enums.js texture.FLOOR = 0, client assumes 1",
            "enums.js has no texture.NOPE (client assumes 3)",
            "enums.js does not publish no_such_group",
            "enums.js mouse_mode = [object Object], client assumes 0",
        ]
    );
}

#[test]
fn reads_the_recorded_fixture() {
    check(&load(&common::fixture_dir()));
}

#[test]
fn reads_the_offline_engines_gamedata() {
    let dir = common::engine_dir();
    if !dir.join("enums.js").exists() {
        eprintln!(
            "skipped: {} is not built (engine/build.sh stable)",
            dir.display()
        );
        return;
    }
    check(&load(&dir));
}
