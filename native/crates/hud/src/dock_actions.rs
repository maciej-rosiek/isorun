//! The dock's action icons: what can be done where the player stands, each a button with crawl's
//! own icon, so nothing hides behind a key. The ones the square offers (the stairs, a shop, an
//! altar, a pile to pick up, an open door to close) come first and are lit; the standing ones
//! (explore or autofight, rest, wait, travel, search) follow.

use crate::gear::KeyStep;
use crate::glyphs::Glyph;
use crate::tooltip::KeyHint;
use crate::Action;
use isorun_scene::{BillboardKind, CellKind, DoorState, Feature, Scene, UpDown, DIR8_DX, DIR8_DY};
use isorun_webtiles::formatted_string_to_text;

/// An icon: a GUI tile by name (crawl's command icons, `CMD_*`), a dungeon feature tile by
/// name, or a tile by the client's id (the feature underfoot).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DockIcon {
    Gui(&'static str),
    Dngn(&'static str),
    Tile(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockAction {
    pub id: &'static str,
    pub label: String,
    pub icon: DockIcon,
    pub action: Action,
    /// offered by the square the player stands on, not always there
    pub here: bool,
    pub key: KeyHint,
}

/// What lies under the player, as the game screen's context reads it (context.ts `targetFor`
/// for the square underfoot): a feature (with the pile on it, if any), a pile, or nothing to act on.
#[derive(Clone, Debug, PartialEq)]
pub enum Underfoot {
    Feature { feature: Feature, label: String, items: Option<String> },
    Item(String),
    Other,
}

/// What the actions are chosen from: the square underfoot and whether a hostile is in view.
#[derive(Clone, Debug, PartialEq)]
pub struct DockContext {
    pub under: Underfoot,
    pub hostiles_in_view: usize,
}

/// context.ts `targetFor` on the square underfoot, for a game screen that has no context of its
/// own to give: the feature with its label, else the pile crawl named underfoot (`items_here`,
/// the message log's own words), else nothing.
pub fn underfoot(scene: &Scene, items_here: Option<&str>) -> Underfoot {
    let Some(cell) = scene.player_on_level.then(|| scene.cell(scene.player.0, scene.player.1)).flatten() else { return Underfoot::Other };
    if cell.kind == CellKind::Unknown {
        return Underfoot::Other;
    }
    let item = items_here.filter(|s| !s.is_empty()).map(String::from);
    match &cell.feature {
        Some(f) if !matches!(f, Feature::Other { .. }) => Underfoot::Feature { feature: f.clone(), label: cell.label.clone().unwrap_or_default(), items: item },
        _ => item.map_or(Underfoot::Other, Underfoot::Item),
    }
}

/// The hostiles in view, as context.ts counts them.
pub fn hostiles_in_view(scene: &Scene) -> usize {
    scene.monsters_in_view().into_iter().filter(|b| b.kind == BillboardKind::Monster && isorun_scene::is_threat(b)).count()
}

fn keys(label: &str, text: &str) -> Action {
    Action::Keys { label: label.into(), seq: vec![KeyStep::text(text)] }
}

fn control(label: &str, key: i32) -> Action {
    Action::Keys { label: label.into(), seq: vec![KeyStep::key(key)] }
}

fn travel_one_level(label: &str, key: &str) -> Action {
    Action::Keys { label: label.into(), seq: vec![KeyStep::text("G"), KeyStep::text(key).awaiting()] }
}

/// The actions for the dock in command mode, in the order they stand.
pub fn dock_actions(ctx: &DockContext, scene: &Scene) -> Vec<DockAction> {
    let mut out = Vec::new();
    let cell = scene.player_on_level.then(|| scene.cell(scene.player.0, scene.player.1)).flatten();
    let here = |id, label: String, icon, action, key| DockAction { id, label, icon, action, here: true, key };
    if let (Underfoot::Feature { feature, label, .. }, Some(tile)) = (&ctx.under, cell.and_then(|c| c.feature_tile)) {
        let icon = DockIcon::Tile(tile);
        let a = KeyHint::keys(&[">"]).pad(Glyph::A);
        match feature {
            Feature::Stairs { dir, exit, .. } => {
                let down = *dir == UpDown::Down;
                let l = if *exit { "Leave the dungeon" } else if down { "Go down" } else { "Go up" };
                out.push(here("stairs", l.into(), icon, keys(l, if down { ">" } else { "<" }), KeyHint::keys(&[if down { ">" } else { "<" }]).pad(Glyph::A)));
            }
            Feature::Hatch { dir } => {
                let down = *dir == UpDown::Down;
                let l = if down { "Go down" } else { "Go up" };
                out.push(here("stairs", l.into(), icon, keys(l, if down { ">" } else { "<" }), KeyHint::keys(&[if down { ">" } else { "<" }]).pad(Glyph::A)));
            }
            Feature::Shop => out.push(here("shop", format!("Enter {label}"), icon, keys("Enter shop", ">"), a)),
            Feature::Altar { .. } => out.push(here("altar", format!("Pray at {label}"), icon, keys("Pray", ">"), a)),
            Feature::Portal | Feature::Transporter => out.push(here("portal", format!("Enter {label}"), icon, keys("Enter", ">"), a)),
            _ => {}
        }
    }
    let (pile, on_item) = match &ctx.under {
        Underfoot::Item(l) => (Some(l.as_str()), true),
        Underfoot::Feature { items: Some(l), .. } => (Some(l.as_str()), false),
        _ => (None, false),
    };
    if let Some(pile) = pile {
        let mut k = KeyHint::keys(&[","]);
        if on_item {
            k = k.pad(Glyph::A);
        }
        out.push(here("pickup", format!("Pick up {}", formatted_string_to_text(pile)), DockIcon::Gui("CMD_DROP"), keys("Pick up", ","), k));
    }
    if scene.player_on_level && open_door_next_to(scene) {
        out.push(here("door", "Close door".into(), DockIcon::Dngn("DNGN_OPEN_DOOR"), keys("Close door", "C"), KeyHint::keys(&["C"])));
    }
    let standing = |id, label: &str, icon, action, key| DockAction { id, label: label.into(), icon, action, here: false, key };
    if ctx.hostiles_in_view > 0 {
        out.push(standing("fight", "Autofight", DockIcon::Gui("CMD_AUTOFIGHT"), control("Autofight", 9), KeyHint::keys(&["Tab"]).pad(Glyph::RT)));
    } else {
        out.push(standing("explore", "Explore", DockIcon::Gui("CMD_EXPLORE"), keys("Explore", "o"), KeyHint::keys(&["o"]).pad(Glyph::LT)));
    }
    out.push(standing("rest", "Rest", DockIcon::Gui("CMD_REST"), keys("Rest", "5"), KeyHint::keys(&["5"]).pad(Glyph::LB).hold()));
    out.push(standing("wait", "Wait a turn", DockIcon::Gui("CMD_WAIT"), keys("Wait", "."), KeyHint::keys(&["."]).pad(Glyph::LB)));
    let on_stairs = out.iter().any(|a| a.id == "stairs");
    let (up, down) = if on_stairs { (false, false) } else { known_stairs(scene) };
    if up {
        out.push(standing("travel-up", "Travel up the stairs", DockIcon::Dngn("DNGN_STONE_STAIRS_UP"), travel_one_level("Travel up", "<"), KeyHint::keys(&["G", "<"])));
    }
    if down {
        out.push(standing("travel-down", "Travel down the stairs", DockIcon::Dngn("DNGN_STONE_STAIRS_DOWN"), travel_one_level("Travel down", ">"), KeyHint::keys(&["G", ">"])));
    }
    out.push(standing("travel", "Travel to a level", DockIcon::Gui("CMD_INTERLEVEL_TRAVEL"), keys("Travel", "G"), KeyHint::keys(&["G"])));
    out.push(standing("search", "Find items and places", DockIcon::Gui("CMD_SEARCH_STASHES"), control("Search", 6), KeyHint::keys(&["Ctrl", "F"])));
    out
}

fn known_stairs(scene: &Scene) -> (bool, bool) {
    let (mut up, mut down) = (false, false);
    for cell in scene.cells.values() {
        if let Some(Feature::Stairs { dir, branch, exit }) = &cell.feature {
            if *dir == UpDown::Up && !exit {
                up = true;
            } else if *dir == UpDown::Down && !branch {
                down = true;
            }
            if up && down {
                break;
            }
        }
    }
    (up, down)
}

fn open_door_next_to(scene: &Scene) -> bool {
    let (x, y) = scene.player;
    (0..8).any(|d| matches!(scene.cell(x + DIR8_DX[d], y + DIR8_DY[d]).and_then(|c| c.feature.as_ref()), Some(Feature::Door { state: DoorState::Open })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use isorun_scene::{cell_key, SceneCell};

    fn scene_with(under: Option<Feature>) -> Scene {
        let mut s = Scene { player_on_level: true, player: (5, 5), ..Default::default() };
        let mut c = SceneCell { x: 5, y: 5, kind: CellKind::Floor, ..Default::default() };
        c.feature = under;
        c.feature_tile = Some(42);
        c.label = Some("stone staircase".into());
        s.cells.insert(cell_key(5, 5), c);
        s
    }

    #[test]
    fn the_square_first_then_the_standing_ones() {
        let s = scene_with(Some(Feature::Stairs { dir: UpDown::Down, branch: false, exit: false }));
        let ctx = DockContext { under: underfoot(&s, Some("a +0 dagger")), hostiles_in_view: 0 };
        let a = dock_actions(&ctx, &s);
        let ids: Vec<&str> = a.iter().map(|a| a.id).collect();
        assert_eq!(ids, ["stairs", "pickup", "explore", "rest", "wait", "travel", "search"]);
        assert_eq!(a[0].label, "Go down");
        assert_eq!(a[0].icon, DockIcon::Tile(42));
        assert!(a[0].here && !a[2].here);
        assert_eq!(a[1].label, "Pick up a +0 dagger");
        // the pile on a feature is not what A takes
        assert_eq!(a[1].key.pad, None);
    }

    #[test]
    fn fight_and_travel() {
        let mut s = scene_with(None);
        s.cells.insert(cell_key(9, 9), SceneCell { x: 9, y: 9, kind: CellKind::Floor, feature: Some(Feature::Stairs { dir: UpDown::Up, branch: false, exit: false }), ..Default::default() });
        let ctx = DockContext { under: underfoot(&s, None), hostiles_in_view: 2 };
        let ids: Vec<&str> = dock_actions(&ctx, &s).iter().map(|a| a.id).collect();
        assert_eq!(ids, ["fight", "rest", "wait", "travel-up", "travel", "search"]);
    }
}
