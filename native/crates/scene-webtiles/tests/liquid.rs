//! The port of liquid.test.ts.

use isorun_scene::{Liquid, LiquidKind};
use isorun_scene_webtiles::{liquid_label, liquid_of};

#[test]
fn tells_every_liquid_and_wet_floor_apart_by_the_tile_and_deep_from_shallow() {
    // given
    let tiles = [
        "DNGN_LAVA_3",
        "DNGN_SHALLOW_WATER_1",
        "DNGN_DEEP_WATER_MURKY",
        "DNGN_OPEN_SEA",
        "DNGN_TOXIC_BOG_2",
        "DNGN_STARWATER_4",
        "LIQUEFACTION_1",
        "FLOOR_SLIME_2",
        "FLOOR_SLIME_ACIDIC",
        "FLOOR_GREY_DIRT",
    ];

    // when
    let liquids: Vec<Option<Liquid>> = tiles.iter().map(|t| liquid_of(Some(t))).collect();

    // then
    let l = |kind, deep| Some(Liquid { kind, deep });
    assert_eq!(
        liquids,
        vec![
            l(LiquidKind::Lava, true),
            l(LiquidKind::Water, false),
            l(LiquidKind::Murky, true),
            l(LiquidKind::Water, true),
            l(LiquidKind::Bog, false),
            l(LiquidKind::Starwater, true),
            l(LiquidKind::Mud, false),
            l(LiquidKind::Slime, false),
            l(LiquidKind::Acid, false),
            None,
        ]
    );
    let labels: Vec<&str> = liquids.iter().flatten().map(liquid_label).collect();
    assert_eq!(labels, ["lava", "shallow water", "deep water", "deep water", "toxic bog", "deep water", "mud", "floor", "corroding ground"]);
    assert_eq!(liquid_of(None), None);
}
