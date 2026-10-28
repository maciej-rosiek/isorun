//! The ports of `cell-info.test.ts`, `cell-menu.test.ts`, `combat.test.ts` and `map-hold.test.ts`.

use isorun_input::cell_info::{cell_details, CellDetail, Tone};
use isorun_input::cell_menu::{cell_menu_rows, CellMenuRun};
use isorun_input::combat::{combat_events, BlowStrength, CombatEvent, Fighter};
use isorun_input::context::{derive_context, Layer, Mode};
use isorun_input::map_hold::MapHold;
use isorun_scene::{cell_key, Attitude, Billboard, BillboardKind, CellKind, Feature, Scene, SceneCell, StatusIcon, Threat, UpDown, Visibility, WoundLevel};
use isorun_webtiles::{initial_state, reduce, GameMessage, GameState, Monster, MouseMode, Phase};
use serde_json::json;
use std::collections::HashMap;

fn floor(x: i32, y: i32) -> SceneCell {
    SceneCell { x, y, kind: CellKind::Floor, visibility: Visibility::Visible, floor_tile: 1, label: Some("floor".into()), ..Default::default() }
}

fn monster(x: i32, y: i32, name: &str, attitude: Attitude, threat: Threat) -> Billboard {
    let mut b = Billboard::new(x, y, 0, BillboardKind::Monster, 1.0);
    b.name = Some(name.into());
    b.attitude = Some(attitude);
    b.threat = Some(threat);
    b
}

fn item(x: i32, y: i32, name: &str) -> Billboard {
    let mut b = Billboard::new(x, y, 0, BillboardKind::Item, 0.5);
    b.name = Some(name.into());
    b
}

// ---------------------------------------------------------------------------
// cell-info.test.ts

#[test]
fn names_the_monster_with_its_threat_and_wounds_and_the_items_under_it() {
    let mut scene = Scene::default();
    scene.cells.insert(cell_key(3, 4), floor(3, 4));
    let mut g = monster(3, 4, "goblin", Attitude::Hostile, Threat::Easy);
    g.damage = Some(WoundLevel::HeavilyDamaged);
    scene.billboards.push(g);
    scene.billboards.push(item(3, 4, "dagger"));
    assert_eq!(
        cell_details(&scene, 3, 4),
        vec![
            CellDetail { text: "Goblin".into(), tone: Some(Tone::Easy), wound: Some(WoundLevel::HeavilyDamaged) },
            CellDetail { text: "easy threat, heavily wounded".into(), tone: Some(Tone::Note), wound: None },
            CellDetail { text: "Dagger".into(), tone: None, wound: None },
        ]
    );
}

#[test]
fn says_when_more_items_lie_under_the_top_item_of_a_pile() {
    let mut scene = Scene::default();
    scene.cells.insert(cell_key(3, 4), floor(3, 4));
    let mut club = item(3, 4, "club");
    club.status_icons.push(StatusIcon { tile: 1, ox: 0.0, oy: 0.0, at_top: false, square: true, full: false });
    scene.billboards.push(club);
    assert_eq!(cell_details(&scene, 3, 4), vec![CellDetail { text: "Club and more items".into(), tone: None, wound: None }]);
}

#[test]
fn says_nothing_about_a_wall() {
    let mut scene = Scene::default();
    scene
        .cells
        .insert(cell_key(3, 4), SceneCell { kind: CellKind::Wall, occluder: true, wall_tile: Some(2), label: Some("brick dark 1".into()), ..floor(3, 4) });
    assert_eq!(cell_details(&scene, 3, 4), vec![]);
}

// ---------------------------------------------------------------------------
// cell-menu.test.ts

fn playing() -> GameState {
    let mut st = initial_state();
    st.phase = Phase::Playing;
    st.input_mode = MouseMode::COMMAND;
    reduce(&mut st, &json!({ "msg": "player", "pos": { "x": 4, "y": 4 }, "turn": 1 }));
    st
}

fn level() -> Scene {
    let mut scene = Scene { player: (4, 4), player_on_level: true, ..Default::default() };
    for x in 0..9 {
        for y in 0..9 {
            scene.cells.insert(cell_key(x, y), floor(x, y));
        }
    }
    scene
}

fn rows_at(scene: &Scene, x: i32, y: i32) -> Vec<(String, CellMenuRun)> {
    let ctx = derive_context(&playing(), scene, 0, Layer::Micro);
    cell_menu_rows(&ctx, scene, x, y).into_iter().map(|r| (r.label, r.run)).collect()
}

#[test]
fn attacks_a_hostile_next_to_the_player_and_describes_it() {
    let mut scene = level();
    scene.billboards.push(monster(5, 4, "goblin", Attitude::Hostile, Threat::Easy));
    assert_eq!(rows_at(&scene, 5, 4), vec![("Attack".into(), CellMenuRun::Click(1)), ("Describe".into(), CellMenuRun::Click(3))]);
}

#[test]
fn only_describes_a_hostile_further_away() {
    let mut scene = level();
    scene.billboards.push(monster(7, 4, "goblin", Attitude::Hostile, Threat::Easy));
    assert_eq!(rows_at(&scene, 7, 4), vec![("Describe".into(), CellMenuRun::Click(3))]);
}

#[test]
fn goes_to_a_cell_with_items_and_describes_them() {
    let mut scene = level();
    scene.billboards.push(item(2, 2, "dagger"));
    assert_eq!(rows_at(&scene, 2, 2).into_iter().map(|r| r.0).collect::<Vec<_>>(), vec!["Go here", "Describe"]);
}

#[test]
fn offers_what_the_players_own_square_offers() {
    let mut scene = level();
    scene.cells.insert(
        cell_key(4, 4),
        SceneCell {
            kind: CellKind::Feature,
            feature: Some(Feature::Stairs { dir: UpDown::Down, branch: false, exit: false }),
            feature_tile: Some(7),
            label: Some("stone staircase leading down".into()),
            ..floor(4, 4)
        },
    );
    assert_eq!(rows_at(&scene, 4, 4).into_iter().map(|r| r.0).collect::<Vec<_>>(), vec!["Go down", "Describe"]);
}

#[test]
fn has_nothing_for_a_cell_never_seen() {
    let mut scene = level();
    scene.cells.insert(cell_key(1, 1), SceneCell { visibility: Visibility::Unseen, ..floor(1, 1) });
    assert_eq!(rows_at(&scene, 1, 1), vec![]);
}

// ---------------------------------------------------------------------------
// combat.test.ts

fn line(text: &str) -> GameMessage {
    GameMessage { text: text.into(), turn: Some(100), ..Default::default() }
}

fn scene_with_goblin() -> (Scene, HashMap<i64, Monster>) {
    let mut scene = Scene { player: (5, 5), ..Default::default() };
    let mut g = Billboard::new(6, 5, 0, BillboardKind::Monster, 1.0);
    g.attitude = Some(Attitude::Hostile);
    g.ref_id = Some(7);
    g.name = Some("goblin".into());
    scene.billboards.push(g);
    let mut monsters = HashMap::new();
    monsters.insert(7, Monster { id: Some(7), name: "goblin".into(), ..Default::default() });
    (scene, monsters)
}

const P: Fighter = Fighter::Player;
const G: Fighter = Fighter::Monster(7);
fn hit(on: Fighter, from: Option<(i32, i32)>, tier: u32, amount: Option<u32>) -> CombatEvent {
    CombatEvent::Hit { on, from, strength: BlowStrength { tier, amount } }
}

#[test]
fn plays_the_players_blows_the_monsters_blows_and_the_kill() {
    let (scene, monsters) = scene_with_goblin();
    let lines = [line("<lightgrey>You slash the goblin.<lightgrey>"), line("The goblin hits you."), line("You kill the goblin!")];
    assert_eq!(
        combat_events(&lines, &scene, &monsters, None),
        vec![
            CombatEvent::Attack { by: P, at: (6, 5) },
            hit(G, Some((5, 5)), 0, None),
            CombatEvent::Attack { by: G, at: (5, 5) },
            hit(P, Some((6, 5)), 0, None),
            CombatEvent::Attack { by: P, at: (6, 5) },
            CombatEvent::Death { on: G, from: Some((5, 5)) },
        ]
    );
}

#[test]
fn plays_a_miss_as_a_swing_and_a_dodge() {
    let (scene, monsters) = scene_with_goblin();
    let lines = [line("You barely miss the goblin."), line("The goblin closely misses you.")];
    assert_eq!(
        combat_events(&lines, &scene, &monsters, None),
        vec![
            CombatEvent::Attack { by: P, at: (6, 5) },
            CombatEvent::Miss { on: G, from: Some((5, 5)) },
            CombatEvent::Attack { by: G, at: (5, 5) },
            CombatEvent::Miss { on: P, from: Some((6, 5)) }
        ]
    );
}

#[test]
fn plays_a_monsters_ranged_attack_and_the_missile_that_hits_the_player() {
    let (scene, monsters) = scene_with_goblin();
    let lines = [line("The goblin breathes fire at you."), line("The searing blast hits you!")];
    assert_eq!(combat_events(&lines, &scene, &monsters, None), vec![CombatEvent::Attack { by: G, at: (5, 5) }, hit(P, None, 1, None)]);
}

#[test]
fn plays_the_blows_on_and_from_a_monster_out_of_sight_where_the_player_struck() {
    let (mut scene, monsters) = scene_with_goblin();
    scene.billboards.clear();
    let lines = [line("You hit something."), line("Something misses you."), line("Something hits you!"), line("You kill something!")];
    let u = Fighter::Unseen((6, 4));
    assert_eq!(
        combat_events(&lines, &scene, &monsters, Some((6, 4))),
        vec![
            CombatEvent::Attack { by: P, at: (6, 4) },
            hit(u, Some((5, 5)), 0, None),
            CombatEvent::Miss { on: P, from: None },
            hit(P, None, 1, None),
            CombatEvent::Attack { by: P, at: (6, 4) },
            CombatEvent::Death { on: u, from: Some((5, 5)) },
        ]
    );
    // with no cell struck at, a blow on something shows nothing; one from it still lands on the player
    assert_eq!(combat_events(&lines, &scene, &monsters, None), vec![CombatEvent::Miss { on: P, from: None }, hit(P, None, 1, None)]);
    // "you see something" is no blow
    assert_eq!(combat_events(&[line("You sense something nearby.")], &scene, &monsters, Some((6, 4))), vec![]);
}

#[test]
fn reads_how_hard_a_blow_struck_and_the_damage_it_did() {
    let (scene, monsters) = scene_with_goblin();
    let lines = [line("You slash the goblin for 12!!"), line("The goblin hits you for 3 with a club.")];
    assert_eq!(
        combat_events(&lines, &scene, &monsters, None),
        vec![
            CombatEvent::Attack { by: P, at: (6, 5) },
            hit(G, Some((5, 5)), 2, Some(12)),
            CombatEvent::Attack { by: G, at: (5, 5) },
            hit(P, Some((6, 5)), 0, Some(3))
        ]
    );
}

// ---------------------------------------------------------------------------
// map-hold.test.ts

#[test]
fn the_map_is_only_crawls_own_when_nothing_is_held() {
    let mut h = MapHold::default();
    assert!(h.shown(true, Mode::Levelmap, 0.0));
    assert!(!h.shown(false, Mode::Command, 0.0));
}

#[test]
fn the_map_stays_until_crawls_map_is_back() {
    let mut h = MapHold::default();
    h.begin(0.0);
    assert!(h.shown(true, Mode::Levelmap, 10.0));
    assert!(h.shown(false, Mode::Command, 20.0));
    assert!(h.shown(false, Mode::Text, 30.0));
    assert!(h.shown(false, Mode::Menu, 40.0));
    assert!(h.shown(true, Mode::Levelmap, 50.0));
    assert!(!h.shown(false, Mode::Command, 60.0));
}

#[test]
fn the_map_lets_go_once_the_screen_it_waited_for_is_put_away() {
    let mut h = MapHold::default();
    h.begin(0.0);
    assert!(h.shown(false, Mode::Popup, 20.0));
    assert!(!h.shown(false, Mode::Command, 30.0));
}

#[test]
fn the_map_lets_go_when_no_screen_came() {
    let mut h = MapHold::default();
    h.begin(0.0);
    assert!(h.shown(false, Mode::Command, 1000.0));
    assert!(!h.shown(false, Mode::Command, 2000.0));
}
