//! The colours of the 2D views that are not tiles: the minimap's colour blocks (drawn until the
//! gamedata's tiles are in, minimap.js's palette from the rc's `tile_*_col` options), the
//! terminal palette of the glyph modes, and the veils over cells out of sight.

use egui::Color32;
use isorun_scene::{Attitude, Beacon, Billboard, BillboardKind, CellKind, Feature, SceneCell, Visibility};
use serde_json::{Map, Value};

/// The colour blocks by what a cell holds (render-2d `DEFAULT_MINIMAP`, the official
/// `tile_*_col` set).
#[derive(Clone, Debug, PartialEq)]
pub struct MinimapColours {
    pub unknown: Color32,
    pub floor: Color32,
    pub wall: Color32,
    pub remembered: Color32,
    pub door: Color32,
    pub feature: Color32,
    pub item: Color32,
    pub monster: Color32,
    pub friendly: Color32,
    pub player: Color32,
    pub water: Color32,
    pub lava: Color32,
    pub up: Color32,
    pub down: Color32,
    pub portal: Color32,
    pub excluded: Color32,
    pub mapped_wall: Color32,
    pub branch_stairs: Color32,
    pub trap: Color32,
    pub transporter: Color32,
    /// The official minimap palette (minimap.js `init_options`): one colour per `map_feature`
    /// index. A cell that carries its server `mf` takes this colour, and the fields above are
    /// only the fallback for cells without one.
    pub mf: Vec<Option<Color32>>,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

impl Default for MinimapColours {
    fn default() -> Self {
        MinimapColours {
            unknown: Color32::TRANSPARENT,
            floor: hex(0x333333),
            wall: hex(0x666666),
            remembered: hex(0x222266),
            door: hex(0xa58c50),
            feature: hex(0x8888ff),
            item: hex(0x66ff66),
            monster: hex(0xff5555),
            friendly: hex(0x55ff55),
            player: hex(0xffffff),
            water: hex(0x0000aa),
            lava: hex(0xaa2200),
            up: hex(0x00ffff),
            down: hex(0xff00ff),
            portal: hex(0xff88ff),
            excluded: hex(0x884444),
            mapped_wall: hex(0x444499),
            branch_stairs: hex(0xffff00),
            trap: hex(0xff0000),
            transporter: hex(0xff88ff),
            mf: Vec::new(),
        }
    }
}

/// minimap.js `init_options`: the rc colour option for each `map_feature` index, in
/// map-feature.h order (MF_UNSEEN first).
pub const MF_OPTION_NAMES: [&str; 27] = [
    "tile_unseen_col",
    "tile_floor_col",
    "tile_wall_col",
    "tile_mapped_floor_col",
    "tile_mapped_wall_col",
    "tile_door_col",
    "tile_item_col",
    "tile_monster_col", // MF_MONS_FRIENDLY
    "tile_monster_col", // MF_MONS_PEACEFUL
    "tile_monster_col", // MF_MONS_NEUTRAL
    "tile_monster_col", // MF_MONS_HOSTILE
    "tile_plant_col",   // MF_MONS_NO_EXP
    "tile_upstairs_col",
    "tile_downstairs_col",
    "tile_branchstairs_col",
    "tile_feature_col",
    "tile_water_col",
    "tile_lava_col",
    "tile_trap_col",
    "tile_excl_centre_col",
    "tile_excluded_col",
    "tile_player_col",
    "tile_deep_water_col",
    "tile_portal_col",
    "tile_transporter_col",
    "tile_transporter_landing_col",
    "tile_explore_horizon_col",
];

/// An rc colour option (`{r, g, b}`), if the server sent it.
fn rc_colour(options: &Map<String, Value>, name: &str) -> Option<Color32> {
    let v = options.get(name)?.as_object()?;
    let c = |k: &str| v.get(k).and_then(|n| n.as_u64()).map(|n| n.min(255) as u8);
    Some(Color32::from_rgb(c("r")?, c("g")?, c("b")?))
}

impl MinimapColours {
    /// The palette from the rc options the server sent (hud.ts `drawMinimap`): every colour the
    /// rc names replaces the default.
    pub fn from_options(options: &Map<String, Value>) -> Self {
        let mut p = MinimapColours::default();
        let set = |slot: &mut Color32, name: &str| {
            if let Some(c) = rc_colour(options, name) {
                *slot = c;
            }
        };
        set(&mut p.floor, "tile_floor_col");
        set(&mut p.wall, "tile_wall_col");
        set(&mut p.remembered, "tile_mapped_floor_col");
        set(&mut p.door, "tile_door_col");
        set(&mut p.item, "tile_item_col");
        set(&mut p.monster, "tile_monster_col");
        set(&mut p.friendly, "tile_monster_col");
        set(&mut p.player, "tile_player_col");
        set(&mut p.water, "tile_water_col");
        set(&mut p.lava, "tile_lava_col");
        set(&mut p.up, "tile_upstairs_col");
        set(&mut p.down, "tile_downstairs_col");
        set(&mut p.portal, "tile_portal_col");
        set(&mut p.feature, "tile_feature_col");
        set(&mut p.excluded, "tile_excluded_col");
        set(&mut p.mapped_wall, "tile_mapped_wall_col");
        set(&mut p.branch_stairs, "tile_branchstairs_col");
        set(&mut p.trap, "tile_trap_col");
        set(&mut p.transporter, "tile_transporter_col");
        p.mf = MF_OPTION_NAMES.iter().map(|n| rc_colour(options, n)).collect();
        p
    }

    /// The block a cell is drawn as (render-2d `drawMinimapCell`): the server's map feature
    /// indexes the rc palette directly; without one, what the scene says the cell is.
    pub fn cell(&self, cell: &SceneCell) -> Color32 {
        if let Some(c) = cell.mf.and_then(|mf| self.mf.get(usize::try_from(mf).ok()?).copied().flatten()) {
            return c;
        }
        let f = cell.feature.as_ref();
        if cell.kind == CellKind::Unknown {
            self.unknown
        } else if cell.flags.excluded {
            self.excluded
        } else if matches!(f, Some(Feature::Stairs { branch: true, .. })) {
            self.branch_stairs
        } else if cell.beacon == Some(Beacon::Up) {
            self.up
        } else if cell.beacon == Some(Beacon::Down) {
            self.down
        } else if matches!(f, Some(Feature::Transporter)) {
            self.transporter
        } else if cell.beacon == Some(Beacon::Portal) {
            self.portal
        } else if matches!(f, Some(Feature::Trap { .. })) {
            self.trap
        } else if cell.kind == CellKind::Door {
            self.door
        } else if cell.kind == CellKind::Feature {
            self.feature
        } else if cell.flags.lava {
            self.lava
        } else if cell.flags.water {
            self.water
        } else if cell.kind == CellKind::Wall {
            if cell.visibility == Visibility::Visible {
                self.wall
            } else {
                self.mapped_wall
            }
        } else if cell.visibility == Visibility::Visible {
            self.floor
        } else {
            self.remembered
        }
    }

    /// The block standing in for a sprite: monsters and items only.
    pub fn billboard(&self, b: &Billboard) -> Option<Color32> {
        match b.kind {
            BillboardKind::Monster if b.attitude == Some(Attitude::Friendly) => Some(self.friendly),
            BillboardKind::Monster => Some(self.monster),
            BillboardKind::Item => Some(self.item),
            _ => None,
        }
    }
}

/// The default terminal palette, as the official stylesheet (render-2d `TERM_COLOURS`).
pub const TERM_COLOURS: [Color32; 16] = [
    hex(0x000000),
    hex(0x0000cc),
    hex(0x00aa00),
    hex(0x00aaaa),
    hex(0xcc0000),
    hex(0xaa00aa),
    hex(0xaa5500),
    hex(0xaaaaaa),
    hex(0x555555),
    hex(0x5555ff),
    hex(0x55ff55),
    hex(0x55ffff),
    hex(0xff5555),
    hex(0xff55ff),
    hex(0xffff55),
    hex(0xffffff),
];

/// `rgba(r, g, b, a)` as egui's premultiplied colour.
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The veil over a cell out of sight: remembered cells are drawn under black at 0.45, those
/// known by magic mapping alone under a dark blue; a visible cell out of range of the
/// current targeting under black at 0.3.
pub fn veil(cell: &SceneCell) -> Option<Color32> {
    if cell.kind == CellKind::Unknown {
        None
    } else if cell.visibility != Visibility::Visible {
        Some(if cell.flags.magic_mapped { rgba(0, 0, 40, 0.45) } else { rgba(0, 0, 0, 0.45) })
    } else if cell.flags.out_of_range {
        Some(rgba(0, 0, 0, 0.3))
    } else {
        None
    }
}

/// A tile drawn at opacity `a`: the texture's colour times white at `a`.
pub fn white_alpha(a: f32) -> Color32 {
    let v = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_premultiplied(v, v, v, v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use isorun_scene::SceneCell;

    #[test]
    fn rc_palette_and_map_features() {
        let mut o = Map::new();
        o.insert("tile_floor_col".into(), serde_json::json!({"r": 1, "g": 2, "b": 3}));
        o.insert("tile_wall_col".into(), serde_json::json!({"r": 9, "g": 9, "b": 9}));
        let p = MinimapColours::from_options(&o);
        assert_eq!(p.floor, Color32::from_rgb(1, 2, 3));
        assert_eq!(p.mf[2], Some(Color32::from_rgb(9, 9, 9)));
        let floor = SceneCell { kind: CellKind::Floor, visibility: Visibility::Visible, ..Default::default() };
        assert_eq!(p.cell(&floor), Color32::from_rgb(1, 2, 3));
        // the server's map feature wins over what the scene makes of the cell
        assert_eq!(p.cell(&SceneCell { mf: Some(2), ..floor.clone() }), Color32::from_rgb(9, 9, 9));
        let remembered = SceneCell { visibility: Visibility::Remembered, ..floor };
        assert_eq!(p.cell(&remembered), hex(0x222266));
    }

    #[test]
    fn veils() {
        let mut c = SceneCell { kind: CellKind::Floor, visibility: Visibility::Visible, ..Default::default() };
        assert_eq!(veil(&c), None);
        c.flags.out_of_range = true;
        assert_eq!(veil(&c), Some(rgba(0, 0, 0, 0.3)));
        c.visibility = Visibility::Remembered;
        assert_eq!(veil(&c), Some(rgba(0, 0, 0, 0.45)));
        c.kind = CellKind::Unknown;
        assert_eq!(veil(&c), None);
    }
}
