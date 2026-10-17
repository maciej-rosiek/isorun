//! The port of packages/gamedata/test/module-numbering.test.ts.

mod common;

use std::collections::HashMap;

use isorun_gamedata::load_gamedata;
use serde_json::json;

/// A tileinfo module as rltiles' tile_list_processor writes it, one 32x32 tile a column.
fn tileinfo(name: &str, prefix: &str, start: Result<(&str, &str), u32>, tiles: &[&str]) -> String {
    let (from, dep) = match start {
        Ok((module, value)) => (format!("m.{value}"), format!("\"./tileinfo-{module}\"")),
        Err(n) => (n.to_string(), String::new()),
    };
    let upper = name.to_uppercase();
    let lines: Vec<String> = tiles
        .iter()
        .map(|t| format!("exports.{t} = val++;"))
        .collect();
    let infos: Vec<String> = (0..tiles.len())
        .map(|i| {
            format!(
                "{{w: 32, h: 32, ox: 0, oy: 0, sx: {}, sy: 0, ex: {}, ey: 32}}",
                i * 32,
                i * 32 + 32
            )
        })
        .collect();
    format!(
        "define([{dep}], function(m) {{\nvar exports = {{}};\nvar val = {from};\n{}\n\
         exports.{upper}_MAX = exports.{prefix}_{upper}_MAX = val++;\nvar tile_info = [{}];\n\
         exports.get_tile_info = function (idx) {{ return tile_info[idx - {from}]; }};\n\
         exports.tile_count = function (idx) {{ return 1; }};\n\
         exports.basetile = function (idx) {{ return idx; }};\n\
         exports.get_img = function (idx) {{ return \"{name}\"; }};\nreturn exports;\n}});",
        lines.join("\n"),
        infos.join(",")
    )
}

/// bcrawl's numbering: main, gui and icons start at 0 again, as DCSS did before it numbered
/// them one after another.
fn bcrawl() -> HashMap<String, String> {
    HashMap::from([
        ("enums.js".into(), common::fixture("enums.js")),
        (
            "tileinfo-floor.js".into(),
            tileinfo("floor", "TILE", Err(0), &["DNGN_UNSEEN", "FLOOR_GREY_DIRT"]),
        ),
        (
            "tileinfo-wall.js".into(),
            tileinfo(
                "wall",
                "TILE",
                Ok(("floor", "TILE_FLOOR_MAX")),
                &["WALL_BRICK_DARK"],
            ),
        ),
        (
            "tileinfo-feat.js".into(),
            tileinfo(
                "feat",
                "TILE",
                Ok(("wall", "TILE_WALL_MAX")),
                &["DNGN_ORCISH_IDOL", "DNGN_ENTER"],
            ),
        ),
        (
            "tileinfo-main.js".into(),
            tileinfo(
                "main",
                "TILE",
                Err(0),
                &["MAIN_FILLER_0", "MONS_RAT", "MONS_GOBLIN"],
            ),
        ),
        (
            "tileinfo-player.js".into(),
            tileinfo(
                "player",
                "TILEP",
                Ok(("main", "TILE_MAIN_MAX")),
                &["BASE_HUMAN"],
            ),
        ),
        (
            "tileinfo-gui.js".into(),
            tileinfo("gui", "TILEG", Err(0), &["GUI_FILLER_0", "CMD_EXPLORE"]),
        ),
        (
            "tileinfo-icons.js".into(),
            tileinfo("icons", "TILEI", Err(0), &["ICONS_FILLER_0", "CURSOR"]),
        ),
    ])
}

#[test]
fn reads_a_server_that_starts_main_gui_and_icons_at_0_again_with_each_tile_from_its_own_atlas() {
    // given
    let files = bcrawl();
    let gd = load_gamedata(
        |name: &str| {
            files
                .get(name)
                .cloned()
                .ok_or_else(|| format!("not published: {name}"))
        },
        "bcrawl",
    )
    .unwrap();

    // when
    let goblin = gd.tile(gd.fg(&json!(2)).value, None).unwrap();
    let explore = gd.tile(gd.gui.id("CMD_EXPLORE").unwrap(), None).unwrap();
    let icons = gd.enums.get("texture", "ICONS").unwrap().to_string();
    let cursor = gd.tile(1, Some(&icons)).unwrap();
    let floor = gd.tile(gd.bg(&json!(1)).value, None).unwrap();

    // then
    assert_eq!((goblin.atlas.as_str(), goblin.sx), ("main", 64));
    assert_eq!((explore.atlas.as_str(), explore.sx), ("gui", 32));
    assert_eq!((cursor.atlas.as_str(), cursor.sx), ("icons", 32));
    assert_eq!((floor.atlas.as_str(), floor.sx), ("floor", 32));
    assert_eq!(gd.main.name(gd.fg(&json!(2)).value), Some("MONS_GOBLIN"));
    // the modules stand one after the other
    let ends: Vec<(i64, i64, i64)> = gd
        .modules()
        .iter()
        .map(|m| (m.first, m.end, m.offset))
        .collect();
    assert_eq!(
        ends,
        [
            (0, 2, 0),
            (2, 3, 0),
            (3, 5, 0),
            (5, 8, 5),
            (8, 9, 5),
            (9, 11, 9),
            (11, 13, 11)
        ]
    );
    assert_eq!(gd.server_tile(2, "main"), 7);
    assert_eq!(gd.server_tile(0, "main"), 0);
    assert_eq!(gd.server_tile_for_texture(1, 6), 12);
    // the status icon sizes are not published: a diagnostic, and the icon's own width
    assert!(!gd.has_status_icon_sizes());
    assert!(gd
        .diagnostics
        .iter()
        .any(|d| d.contains("status-icon-sizes.js not available")));
    assert_eq!(gd.status_icon_size(12), 32);
}
