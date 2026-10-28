//! The isometric test bed's level (`native/crates/client/assets/iso.des`) built from its packed atlas
//! (`atlas.json` beside it), as the native test bed builds it.

use std::path::PathBuf;

use isorun_scene::{Attitude, Beacon, BillboardKind, CellKind, DoorState, Feature, LiquidKind, Sky, Stance, UpDown, Visibility, WallStyle};
use isorun_vault::*;

fn repo(path: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..").join(path);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn testbed() -> (Vault, PackedAtlas) {
    let vault = parse_des(&repo("native/crates/client/assets/iso.des")).unwrap();
    let atlas = PackedAtlas::from_json(&repo("native/crates/client/assets/atlas.json")).unwrap();
    (vault, atlas)
}

/// Every cell of the map holding this glyph.
fn at(v: &Vault, g: char) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for (y, row) in v.map.iter().enumerate() {
        for (x, &c) in row.iter().enumerate() {
            if c == g {
                out.push((x as i32, y as i32));
            }
        }
    }
    out
}

#[test]
fn reads_the_packed_atlas_as_the_room_tiles_do() {
    let (_, atlas) = testbed();
    let wall = atlas.id("WALL_BRICK_DARK").unwrap();
    assert_eq!(wall, 1104);
    assert_eq!(atlas.tile_count(wall), 12);
    assert_eq!(atlas.tile_count(999_999), 1);
    assert_eq!(atlas.id("NOT_A_TILE"), None);
    let rect = atlas.tile(2).unwrap();
    assert_eq!((rect.atlas.as_str(), rect.sx, rect.sy, rect.w, rect.h, rect.cell), (PACKED_ATLAS, 0, 0, 32, 32, 32));
    assert_eq!(atlas.atlas_names(), [PACKED_ATLAS]);
    // the sprites are the tiles from crawl's feature, item and monster atlases, not its floors and walls
    let sprites = atlas.sprite_rects(PACKED_ATLAS).unwrap();
    let from_sprites = atlas.json.tiles.values().filter(|t| ["feat", "main", "player"].contains(&t.from.as_str())).count();
    assert_eq!(sprites.len(), from_sprites);
    assert!(sprites.len() < atlas.json.tiles.len());
    assert!(atlas.sprite_rects("floor").is_none());
}

#[test]
fn builds_the_test_bed_level_from_its_atlas() {
    let (v, atlas) = testbed();
    let s = vault_scene(&v, &atlas).unwrap();
    assert_eq!((s.bounds.right, s.bounds.bottom), (v.width as i32 - 1, v.height as i32 - 1));
    assert_eq!(s.cells.len(), v.map.iter().flatten().filter(|g| **g != ' ').count());
    assert!(s.cells.values().all(|c| c.visibility == Visibility::Visible));
    // every tile the scene names is in the atlas, the variants among them
    for c in s.cells.values() {
        for t in [Some(c.floor_tile), c.wall_tile, c.feature_tile].into_iter().flatten() {
            assert!(atlas.tile(t).is_some(), "tile {t} at {},{}", c.x, c.y);
        }
    }
    for b in &s.billboards {
        assert!(atlas.tile(b.tile).is_some(), "tile {} at {},{}", b.tile, b.x, b.y);
    }

    // walls: the brick, in its variants, and the lid
    let brick = atlas.id("WALL_BRICK_DARK").unwrap();
    let wall = s.cell(0, 0).unwrap();
    assert_eq!(wall.kind, CellKind::Wall);
    assert!(wall.occluder);
    assert_eq!(wall.wall_style, Some(WallStyle::Solid));
    assert_eq!(wall.label.as_deref(), Some("brick dark"));
    let masonry = at(&v, '#');
    let walls: Vec<_> = s.cells.values().filter(|c| c.kind == CellKind::Wall && masonry.contains(&(c.x, c.y))).collect();
    assert!(walls.iter().all(|c| (brick..brick + 12).contains(&c.wall_tile.unwrap())));
    assert!(walls.iter().map(|c| c.wall_tile).collect::<std::collections::HashSet<_>>().len() > 3);
    assert_eq!(s.level.ceiling_tile, Some(brick));
    assert_eq!(s.level.sky, Sky::None);

    // a patch of each liquid, by its floor tile
    for (g, kind, deep, label) in [
        ('w', LiquidKind::Water, true, "deep water"),
        ('s', LiquidKind::Water, false, "shallow water"),
        ('L', LiquidKind::Lava, true, "lava"),
        ('m', LiquidKind::Murky, true, "deep water"),
        ('b', LiquidKind::Bog, false, "toxic bog"),
        ('*', LiquidKind::Starwater, true, "deep water"),
        ('u', LiquidKind::Mud, false, "mud"),
        ('z', LiquidKind::Slime, false, "floor"),
        ('a', LiquidKind::Acid, false, "corroding ground"),
    ] {
        let cells = at(&v, g);
        assert!(!cells.is_empty(), "{g}");
        for (x, y) in cells {
            let c = s.cell(x, y).unwrap();
            assert_eq!(c.kind, CellKind::Floor);
            let l = c.liquid.unwrap();
            assert_eq!((l.kind, l.deep), (kind, deep), "{g}");
            assert_eq!(c.label.as_deref(), Some(label));
            assert_eq!(c.flags.water, matches!(kind, LiquidKind::Water | LiquidKind::Murky | LiquidKind::Starwater));
            assert_eq!(c.flags.lava, kind == LiquidKind::Lava);
        }
    }
    let lava = atlas.id("DNGN_LAVA").unwrap();
    let (lx, ly) = at(&v, 'L')[0];
    assert!((lava..lava + 16).contains(&s.cell(lx, ly).unwrap().floor_tile));
    let dry = s.cell(1, 1).unwrap();
    assert_eq!((dry.kind, dry.liquid), (CellKind::Floor, None));

    // doors: a closed one stands as masonry, an open one is a feature
    let closed = at(&v, '+');
    assert_eq!(closed.len(), 4);
    for (x, y) in closed {
        let d = s.cell(x, y).unwrap();
        assert_eq!(d.kind, CellKind::Door);
        assert!(d.occluder);
        assert_eq!(d.feature, Some(Feature::Door { state: DoorState::Closed }));
        assert_eq!(d.wall_tile, atlas.id("DNGN_CLOSED_DOOR"));
        assert_eq!(d.label.as_deref(), Some("closed door"));
    }
    let (ox, oy) = at(&v, '\'')[0];
    let open = s.cell(ox, oy).unwrap();
    assert_eq!(open.kind, CellKind::Feature);
    assert!(!open.occluder);
    assert_eq!(open.feature, Some(Feature::Door { state: DoorState::Open }));
    assert_eq!(open.stance, Some(Stance::Upright));
    assert!(!open.freestanding);

    // the statue stands free in its cell, the stairs lie in the floor
    for (x, y) in at(&v, '8') {
        let statue = s.cell(x, y).unwrap();
        assert_eq!(statue.kind, CellKind::Feature);
        assert_eq!(statue.feature_tile, atlas.id("DNGN_GRANITE_STATUE"));
        assert_eq!(statue.stance, Some(Stance::Upright));
        assert!(statue.freestanding);
        assert_eq!(statue.label.as_deref(), Some("granite statue"));
    }
    let (sx, sy) = at(&v, '>')[0];
    let stairs = s.cell(sx, sy).unwrap();
    assert_eq!(stairs.feature, Some(Feature::Stairs { dir: UpDown::Down, branch: false, exit: false }));
    assert_eq!((stairs.stance, stairs.beacon), (Some(Stance::Lying), Some(Beacon::Down)));

    // monsters and items stand as billboards
    let monsters: Vec<_> = s.billboards.iter().filter(|b| b.kind == BillboardKind::Monster).collect();
    assert_eq!(monsters.len(), 10);
    assert!(monsters.iter().all(|b| b.scenery && b.attitude == Some(Attitude::Hostile)));
    assert!(monsters.iter().all(|b| b.height == if b.name.as_deref() == Some("fungus") { 0.95 } else { 0.6 }));
    let (hx, hy) = at(&v, 'h')[0];
    let human = monsters.iter().find(|b| (b.x, b.y) == (hx, hy)).unwrap();
    assert_eq!(human.name.as_deref(), Some("human"));
    let h = atlas.id("MONS_HUMAN").unwrap();
    assert!((h..h + 3).contains(&human.tile));
    let (kx, ky) = at(&v, 'K')[0];
    let skeleton = monsters.iter().find(|b| (b.x, b.y) == (kx, ky)).unwrap();
    assert_eq!((skeleton.name.as_deref(), Some(skeleton.tile)), (Some("skeletal warrior"), atlas.id("MONS_SKELETAL_WARRIOR")));
    let items: Vec<_> = s.billboards.iter().filter(|b| b.kind == BillboardKind::Item).collect();
    assert_eq!(items.len(), 4);
    for (g, tile) in [('!', "POTION_OFFSET"), ('?', "SCROLL"), ('$', "GOLD05")] {
        let (x, y) = at(&v, g)[0];
        let item = items.iter().find(|b| (b.x, b.y) == (x, y)).unwrap();
        assert_eq!(Some(item.tile), atlas.id(tile));
        assert_eq!(item.height, 0.4);
        // the cell under a billboard is floor
        assert_eq!(s.cell(x, y).unwrap().kind, CellKind::Floor);
    }

    // the eye faces north from its floor; the light is the player
    assert_eq!(v.eye, Eye { x: at(&v, '@')[0].0, y: at(&v, '@')[0].1, facing: 0 });
    assert_eq!(s.player, at(&v, 'l')[0]);
    assert_eq!(s.cell_under().unwrap().kind, CellKind::Floor);
}
